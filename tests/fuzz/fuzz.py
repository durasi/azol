#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""A differential fuzzer for the azoL compiler.

It writes random azoL programs that are valid, end, and always behave the
same way; works out what each one must print with a small interpreter of
its own (the oracle); builds the program for x86-64 Linux and runs it;
builds it for arm64 macOS and runs it under the checking emulator when
that is available; and compares the three. A program whose results differ,
which does not compile, crashes, hangs or breaks a heap rule is a finding.
Findings are shrunk automatically and saved under `findings/`.

    python3 tests/fuzz/fuzz.py --count 3000 --minutes 25
    python3 tests/fuzz/fuzz.py --show 1234          print program 1234
    python3 tests/fuzz/fuzz.py --only 1234          run program 1234 alone

See README.md next to this file.
"""
import argparse
import copy
import os
import random
import re
import subprocess
import sys
import tempfile
import time
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
sys.setrecursionlimit(20000)

# ---------------------------------------------------------------- types

I64 = ('i64',)
BOOL = ('bool',)
STR = ('str',)


def TList(t):
    return ('list', t)


def TStruct(name, *args):
    return ('struct', name, tuple(args))


def TEnum(name, *args):
    return ('enum', name, tuple(args))


def TOpt(t):
    return TEnum('Option', t)


def TRes(t, e):
    return TEnum('Result', t, e)


def TP(name):
    return ('tp', name)


def ty(t):
    k = t[0]
    if k in ('i64', 'bool', 'str'):
        return k
    if k == 'list':
        return 'List[' + ty(t[1]) + ']'
    if k == 'tp':
        return t[1]
    return t[1] + ('[' + ', '.join(ty(x) for x in t[2]) + ']' if t[2] else '')


def subst(t, m):
    k = t[0]
    if k == 'tp':
        return m[t[1]]
    if k == 'list':
        return ('list', subst(t[1], m))
    if k in ('struct', 'enum'):
        return (k, t[1], tuple(subst(x, m) for x in t[2]))
    return t


class Decls:
    """The struct and enum declarations of one program."""

    def __init__(self):
        self.structs = {}   # name -> (type parameters, [(field, type)])
        self.enums = {}     # name -> (type parameters, [(variant, [types])])
        self.order = []     # ('struct' | 'enum', name) in declaration order
        self.enums['Option'] = (['T'], [('Some', [TP('T')]), ('None', [])])
        self.enums['Result'] = (['T', 'E'], [('Ok', [TP('T')]), ('Err', [TP('E')])])

    def add_struct(self, name, params, fields):
        self.structs[name] = (params, fields)
        self.order.append(('struct', name))

    def add_enum(self, name, params, variants):
        self.enums[name] = (params, variants)
        self.order.append(('enum', name))

    def fields(self, t):
        params, fields = self.structs[t[1]]
        m = dict(zip(params, t[2]))
        return [(f, subst(ft, m)) for f, ft in fields]

    def variants(self, t):
        params, variants = self.enums[t[1]]
        m = dict(zip(params, t[2]))
        return [(v, [subst(p, m) for p in ps]) for v, ps in variants]

    def unit_only(self, t):
        return t[0] == 'enum' and all(not ps for _, ps in self.enums[t[1]][1])


# ------------------------------------------------------------------ AST

class E:
    """An expression: a kind, its type, and the parts."""
    __slots__ = ('k', 't', 'a')

    def __init__(self, k, t, *a):
        self.k = k
        self.t = t
        self.a = list(a)


class Fn:
    def __init__(self, name, tparams, params, ret, body, fixed=False):
        self.name = name
        self.tparams = tparams
        self.params = params    # [(name, type, is_mut)]
        self.ret = ret          # a type or None
        self.body = body
        self.fixed = fixed      # a hand-written helper the reducer leaves alone
        self.rec = False


class Program:
    def __init__(self):
        self.decls = Decls()
        self.consts = []        # [(name, type, E)]
        self.fns = []
        self.seed = 0


def kids(e):
    """The expressions directly inside an expression."""
    k = e.k
    a = e.a
    if k in ('bin',):
        return [a[1], a[2]]
    if k in ('and', 'or', 'index'):
        return [a[0], a[1]]
    if k in ('neg', 'not', 'mut'):
        return [a[0]]
    if k == 'field':
        return [a[0]]
    if k in ('call', 'builtin'):
        return list(a[1])
    if k == 'slit':
        return [x for _, x in a[1]]
    if k == 'llit':
        return list(a[0])
    if k == 'ctor':
        return list(a[1])
    if k == 'm':
        return [a[1]] + list(a[2])
    return []


def has_slit(e):
    return e.k == 'slit' or any(has_slit(x) for x in kids(e))


# ------------------------------------------------------------- printing

PREC = {'*': 10, '/': 10, '%': 10, '+': 9, '-': 9, '<<': 8, '>>': 8, '&': 7, '^': 6, '|': 5,
        '==': 4, '!=': 4, '<': 4, '<=': 4, '>': 4, '>=': 4}
CMP = ('==', '!=', '<', '<=', '>', '>=')
MIN = -(1 << 63)
MAX = (1 << 63) - 1
M64 = (1 << 64) - 1


def int_text(v, style):
    if v == MIN:
        return '0x8000000000000000', 12
    if style == 'c' and 32 <= v <= 126:
        c = chr(v)
        if c in "'\\":
            c = '\\' + c
        return "'" + c + "'", 12
    if style == 'x':
        return hex(v & M64), 12
    if style == 'b' and 0 <= v < (1 << 20):
        return bin(v), 12
    if style == 'u' and v >= 0:
        return '{:_}'.format(v), 12
    return str(v), (12 if v >= 0 else 11)


def str_text(b):
    out = ['"']
    for c in b:
        if c == 34:
            out.append('\\"')
        elif c == 92:
            out.append('\\\\')
        elif c == 10:
            out.append('\\n')
        elif c == 9:
            out.append('\\t')
        elif c == 13:
            out.append('\\r')
        elif c == 0:
            out.append('\\0')
        else:
            out.append(chr(c))
    out.append('"')
    return ''.join(out)


def rx(e, p=0):
    """The text of an expression that sits where precedence `p` is needed."""
    k = e.k
    a = e.a
    q = 12
    if k == 'int':
        s, q = int_text(a[0], a[1])
    elif k == 'bool':
        s = 'true' if a[0] else 'false'
    elif k == 'str':
        s = str_text(a[0])
    elif k in ('var', 'const'):
        s = a[0]
    elif k == 'bin':
        op = a[0]
        q = PREC[op]
        if op in CMP:
            s = rx(a[1], q + 1) + ' ' + op + ' ' + rx(a[2], q + 1)
        else:
            s = rx(a[1], q) + ' ' + op + ' ' + rx(a[2], q + 1)
    elif k == 'and':
        q = 3
        s = rx(a[0], 3) + ' && ' + rx(a[1], 4)
    elif k == 'or':
        q = 2
        s = rx(a[0], 2) + ' || ' + rx(a[1], 3)
    elif k == 'neg':
        q = 11
        s = '-' + rx(a[0], 12)
    elif k == 'not':
        q = 11
        s = '!' + rx(a[0], 12)
    elif k == 'mut':
        s = 'mut ' + rx(a[0])
    elif k in ('call', 'builtin'):
        s = a[0] + '(' + ', '.join(rx(x) for x in a[1]) + ')'
    elif k == 'field':
        s = rx(a[0], 12) + '.' + a[1]
    elif k == 'index':
        s = rx(a[0], 12) + '[' + rx(a[1]) + ']'
    elif k == 'm':
        s = rx(a[1], 12) + '.' + a[0] + '(' + ', '.join(rx(x) for x in a[2]) + ')'
    elif k == 'slit':
        q = 11
        s = a[0] + ' { ' + ', '.join(f + ': ' + rx(x) for f, x in a[1]) + ' }'
    elif k == 'llit':
        s = '[' + ', '.join(rx(x) for x in a[0]) + ']'
    elif k == 'ctor':
        s = a[0] + ('(' + ', '.join(rx(x) for x in a[1]) + ')' if a[1] else '')
    else:
        raise ValueError(k)
    if q < p:
        s = '(' + s + ')'
    return s


def rtop(e, pad):
    """An expression that ends a statement; a long one continues on the
    next line after its last operator."""
    s = rx(e)
    if len(s) + len(pad) <= 110 or e.k not in ('bin', 'and', 'or'):
        return s
    a = e.a
    if e.k == 'bin':
        op = a[0]
        q = PREC[op]
        left = rx(a[1], q + 1 if op in CMP else q)
        right = rx(a[2], q + 1)
    elif e.k == 'and':
        op, left, right = '&&', rx(a[0], 3), rx(a[1], 4)
    else:
        op, left, right = '||', rx(a[0], 2), rx(a[1], 3)
    return left + ' ' + op + '\n' + pad + '        ' + right


def rcond(e):
    """An expression where a struct literal needs parentheses around it."""
    s = rx(e)
    if has_slit(e):
        return '(' + s + ')'
    return s


def rpat(p):
    if p[0] == 'v':
        return p[1] + ('(' + ', '.join(p[2]) + ')' if p[2] else '')
    if p[0] == 'lit':
        return rx(p[1])
    if p[0] == 'bind':
        return p[1]
    return '_'


SIMPLE = ('print', 'assign', 'expr', 'return', 'break', 'continue', 'eprint', 'exit')


def rs(st, ind, out):
    pad = '    ' * ind
    k = st[0]
    if k == 'let':
        out.append(pad + ('var ' if st[2] else 'let ') + st[1] + (': ' + ty(st[4]) if st[3] else '') + ' = ' + rtop(st[5], pad))
    elif k == 'assign':
        out.append(pad + rx(st[1]) + ' = ' + rtop(st[2], pad))
    elif k in ('print', 'eprint', 'exit'):
        out.append(pad + k + '(' + rx(st[1]) + ')')
    elif k == 'expr':
        out.append(pad + rx(st[1]))
    elif k == 'if':
        for i, (cond, blk) in enumerate(st[1]):
            head = (pad + 'if ' if i == 0 else pad + '} else if ') + rcond(cond) + ' {'
            out.append(head)
            rblock(blk, ind + 1, out)
        if st[2] is not None:
            out.append(pad + '} else {')
            rblock(st[2], ind + 1, out)
        out.append(pad + '}')
    elif k == 'while':
        out.append(pad + 'while ' + rcond(st[1]) + ' {')
        rblock(st[2], ind + 1, out)
        out.append(pad + '}')
    elif k == 'for':
        names = (st[1] + ', ' if st[1] else '') + st[2]
        out.append(pad + 'for ' + names + ' in ' + rcond(st[3]) + ' {')
        rblock(st[4], ind + 1, out)
        out.append(pad + '}')
    elif k in ('break', 'continue'):
        out.append(pad + k)
    elif k == 'match':
        out.append(pad + 'match ' + rcond(st[1]) + ' {')
        for pat, blk, inline in st[2]:
            head = '    ' * (ind + 1) + rpat(pat) + ' => '
            if inline and len(blk) == 1 and blk[0][0] in SIMPLE:
                one = []
                rs(blk[0], ind + 1, one)
                out.append(head + one[0].lstrip())
            else:
                out.append(head + '{')
                rblock(blk, ind + 2, out)
                out.append('    ' * (ind + 1) + '}')
        out.append(pad + '}')
    elif k == 'return':
        out.append(pad + 'return' + (' ' + rtop(st[1], pad) if st[1] is not None else ''))
    else:
        raise ValueError(k)


def rblock(blk, ind, out):
    for st in blk:
        rs(st, ind, out)


def render(prog):
    out = ['// Written by tests/fuzz/fuzz.py, seed ' + str(prog.seed) + '.', '']
    d = prog.decls
    for kind, name in d.order:
        if kind == 'struct':
            params, fields = d.structs[name]
            out.append('struct ' + name + ('[' + ', '.join(params) + ']' if params else '') + ' {')
            for f, ft in fields:
                out.append('    ' + f + ': ' + ty(ft))
            out.append('}')
        else:
            params, variants = d.enums[name]
            out.append('enum ' + name + ('[' + ', '.join(params) + ']' if params else '') + ' {')
            for v, ps in variants:
                out.append('    ' + v + ('(' + ', '.join(ty(p) for p in ps) + ')' if ps else ''))
            out.append('}')
        out.append('')
    for name, _, e in prog.consts:
        out.append('const ' + name + ' = ' + rx(e))
    if prog.consts:
        out.append('')
    for f in prog.fns:
        ps = ', '.join(n + ': ' + ('mut ' if m else '') + ty(t) for n, t, m in f.params)
        head = 'fn ' + f.name + ('[' + ', '.join(f.tparams) + ']' if f.tparams else '') + '(' + ps + ')'
        if f.ret is not None:
            head += ' -> ' + ty(f.ret)
        out.append(head + ' {')
        rblock(f.body, 1, out)
        out.append('}')
        out.append('')
    return '\n'.join(out)


# --------------------------------------------------------------- oracle

class Stop(Exception):
    """The program stops with a run-time error."""

    def __init__(self, msg):
        self.msg = msg


class ExitProg(Exception):
    def __init__(self, code):
        self.code = code


class Budget(Exception):
    """The program does more work than the fuzzer wants to wait for."""


class OracleError(Exception):
    """The program is not valid (an unknown name, a missing match arm)."""


class EnumV:
    __slots__ = ('name', 'vals')

    def __init__(self, name, vals):
        self.name = name
        self.vals = vals


class Cell:
    __slots__ = ('v',)

    def __init__(self, v):
        self.v = v

    def get(self):
        return self.v

    def set(self, v):
        self.v = v


class PlaceCell:
    """A `mut` argument that is a field or an element."""
    __slots__ = ('obj', 'key')

    def __init__(self, obj, key):
        self.obj = obj
        self.key = key

    def get(self):
        return self.obj[self.key]

    def set(self, v):
        self.obj[self.key] = v


def wrap(x):
    x &= M64
    return x - (1 << 64) if x >> 63 else x


def same(a, b):
    if isinstance(a, EnumV):
        return a.name == b.name
    return a == b


BRK = 'brk'
CNT = 'cnt'


class Interp:
    """Runs a program from its tree with exact 64-bit arithmetic."""

    def __init__(self, prog, limit=120000):
        self.prog = prog
        self.fns = {f.name: f for f in prog.fns}
        self.consts = {}
        self.out = []
        self.err = []
        self.steps = 0
        self.limit = limit
        self.depth = 0

    def run(self):
        """Returns (stdout, stderr, exit code)."""
        try:
            for name, _, e in self.prog.consts:
                self.consts[name] = self.ev(e, [])
            v = self.invoke(self.fns['main'], [])
            code = (v & 255) if isinstance(v, int) else 0
        except Stop as s:
            self.err.append(s.msg + b'\n')
            code = 1
        except ExitProg as x:
            code = x.code
        except (KeyError, IndexError, TypeError, AttributeError) as x:
            raise OracleError(repr(x))
        return b''.join(self.out), b''.join(self.err), code

    def cp(self, v):
        """A deep copy that counts as work, so huge values end the run."""
        if isinstance(v, bytes):
            self.steps += len(v) >> 6
            return v
        if isinstance(v, list):
            self.steps += len(v) + 1
            if self.steps > self.limit:
                raise Budget()
            return [self.cp(x) for x in v]
        if isinstance(v, dict):
            self.steps += 1
            return {k: self.cp(x) for k, x in v.items()}
        if isinstance(v, EnumV):
            self.steps += 1
            return EnumV(v.name, [self.cp(x) for x in v.vals])
        return v

    def tick(self):
        self.steps += 1
        if self.steps > self.limit:
            raise Budget()

    def look(self, env, name):
        for sc in reversed(env):
            c = sc.get(name)
            if c is not None:
                return c
        raise OracleError('unknown name ' + name)

    def invoke(self, f, cells):
        self.depth += 1
        if self.depth > 300:
            raise Budget()
        env = [dict((p[0], c) for p, c in zip(f.params, cells))]
        r = self.block(f.body, env)
        self.depth -= 1
        if isinstance(r, tuple):
            return r[1]
        return None

    def place_cell(self, p, env):
        if p.k == 'var':
            return self.look(env, p.a[0])
        if p.k == 'field':
            return PlaceCell(self.ev(p.a[0], env), p.a[1])
        if p.k == 'index':
            i = self.ev(p.a[1], env)
            obj = self.ev(p.a[0], env)
            if not 0 <= i < len(obj):
                raise Stop(b'azol: index out of range')
            return PlaceCell(obj, i)
        raise OracleError('not a place')

    def binop(self, op, l, r):
        if op == '+':
            if isinstance(l, bytes):
                if len(l) + len(r) > 60000:
                    raise Budget()
                return l + r
            return wrap(l + r)
        if op == '-':
            return wrap(l - r)
        if op == '*':
            return wrap(l * r)
        if op == '/' or op == '%':
            if r == 0:
                raise Stop(b'azol: division by zero')
            q = abs(l) // abs(r)
            if (l < 0) != (r < 0):
                q = -q
            if op == '/':
                return wrap(q)
            return wrap(l - q * r)
        if op == '<<':
            return wrap(l << (r & 63))
        if op == '>>':
            return l >> (r & 63)
        if op == '&':
            return l & r
        if op == '|':
            return l | r
        if op == '^':
            return l ^ r
        if op == '==':
            return same(l, r)
        if op == '!=':
            return not same(l, r)
        if op == '<':
            return l < r
        if op == '<=':
            return l <= r
        if op == '>':
            return l > r
        if op == '>=':
            return l >= r
        raise OracleError(op)

    def ev(self, e, env):
        self.tick()
        k = e.k
        a = e.a
        if k == 'var':
            return self.look(env, a[0]).get()
        if k == 'int' or k == 'bool' or k == 'str':
            return a[0]
        if k == 'bin':
            l = self.ev(a[1], env)
            r = self.ev(a[2], env)
            return self.binop(a[0], l, r)
        if k == 'and':
            return self.ev(a[0], env) and self.ev(a[1], env)
        if k == 'or':
            return self.ev(a[0], env) or self.ev(a[1], env)
        if k == 'neg':
            return wrap(-self.ev(a[0], env))
        if k == 'not':
            return not self.ev(a[0], env)
        if k == 'const':
            return self.consts[a[0]]
        if k == 'call':
            cells = []
            for x in a[1]:
                if x.k == 'mut':
                    cells.append(self.place_cell(x.a[0], env))
                else:
                    cells.append(Cell(self.ev(x, env)))
            return self.invoke(self.fns[a[0]], cells)
        if k == 'field':
            return self.ev(a[0], env)[a[1]]
        if k == 'index':
            i = self.ev(a[1], env)
            obj = self.ev(a[0], env)
            if not 0 <= i < len(obj):
                raise Stop(b'azol: index out of range')
            return obj[i]
        if k == 'slit':
            return dict((f, self.cp(self.ev(x, env))) for f, x in a[1])
        if k == 'llit':
            return [self.cp(self.ev(x, env)) for x in a[0]]
        if k == 'ctor':
            return EnumV(a[0], [self.cp(self.ev(x, env)) for x in a[1]])
        if k == 'm':
            return self.method(a[0], a[1], a[2], env)
        if k == 'builtin':
            v = self.ev(a[1][0], env)
            if a[0] == 'str':
                if v is True:
                    return b'true'
                if v is False:
                    return b'false'
                return str(v).encode()
            if a[0] == 'chr':
                if not 0 <= v <= 255:
                    raise Stop(b'azol: chr needs a number from 0 to 255')
                return bytes([v])
        raise OracleError('expression ' + k)

    def method(self, name, recv_e, arg_es, env):
        if name == 'push':
            v = self.cp(self.ev(arg_es[0], env))
            lst = self.ev(recv_e, env)
            lst.append(v)
            if len(lst) > 4000:
                raise Budget()
            return None
        recv = self.ev(recv_e, env)
        args = [self.ev(x, env) for x in arg_es]
        if name == 'len':
            return len(recv)
        if name == 'pop':
            if not recv:
                raise Stop(b'azol: pop from an empty list')
            return recv.pop()
        if name == 'slice':
            s, t = args
            if not (0 <= t <= len(recv) and 0 <= s <= t):
                raise Stop(b'azol: slice out of range')
            return recv[s:t]
        if name == 'find':
            at = recv.find(args[0])
            return EnumV('Some', [at]) if at >= 0 else EnumV('None', [])
        if name == 'contains':
            if isinstance(recv, bytes):
                return args[0] in recv
            return any(same(x, args[0]) for x in recv)
        if name == 'starts_with':
            return recv.startswith(args[0])
        if name == 'ends_with':
            return recv.endswith(args[0])
        if name == 'trim':
            return recv.strip(b' \t\n\r')
        if name == 'split':
            if not args[0]:
                return [recv]
            return recv.split(args[0])
        if name == 'lines':
            parts = recv.split(b'\n')
            last = parts.pop()
            lines = [p[:-1] if p.endswith(b'\r') else p for p in parts]
            if last:
                lines.append(last)
            return lines
        if name == 'to_i64':
            s = recv
            i = 0
            negative = False
            if s and s[0] == 45:
                negative = True
                i = 1
            if i == len(s):
                return EnumV('None', [])
            value = 0
            while i < len(s):
                digit = s[i] - 48
                if digit < 0 or digit > 9:
                    return EnumV('None', [])
                if value > (MAX - digit) // 10:
                    return EnumV('None', [])
                value = value * 10 + digit
                i += 1
            return EnumV('Some', [-value if negative else value])
        if name == 'join':
            if sum(len(x) for x in recv) + len(args[0]) * len(recv) > 60000:
                raise Budget()
            return args[0].join(recv)
        if name == 'first':
            return EnumV('Some', [self.cp(recv[0])]) if recv else EnumV('None', [])
        if name == 'last':
            return EnumV('Some', [self.cp(recv[-1])]) if recv else EnumV('None', [])
        if name == 'reversed':
            return [self.cp(x) for x in reversed(recv)]
        raise OracleError('method ' + name)

    def emit(self, v):
        if v is True:
            b = b'true'
        elif v is False:
            b = b'false'
        elif isinstance(v, bytes):
            b = v
        else:
            b = str(v).encode()
        self.out.append(b + b'\n')
        if len(self.out) > 3000:
            raise Budget()

    def block(self, stmts, env):
        env.append({})
        try:
            for st in stmts:
                r = self.stmt(st, env)
                if r is not None:
                    return r
        finally:
            env.pop()
        return None

    def stmt(self, st, env):
        self.tick()
        k = st[0]
        if k == 'let':
            env[-1][st[1]] = Cell(self.cp(self.ev(st[5], env)))
        elif k == 'assign':
            v = self.cp(self.ev(st[2], env))
            t = st[1]
            if t.k == 'var':
                self.look(env, t.a[0]).set(v)
            elif t.k == 'field':
                self.ev(t.a[0], env)[t.a[1]] = v
            elif t.k == 'index':
                i = self.ev(t.a[1], env)
                obj = self.ev(t.a[0], env)
                if not 0 <= i < len(obj):
                    raise Stop(b'azol: index out of range')
                obj[i] = v
            else:
                raise OracleError('assignment target')
        elif k == 'print':
            self.emit(self.ev(st[1], env))
        elif k == 'expr':
            self.ev(st[1], env)
        elif k == 'if':
            for cond, blk in st[1]:
                if self.ev(cond, env):
                    return self.block(blk, env)
            if st[2] is not None:
                return self.block(st[2], env)
        elif k == 'while':
            while self.ev(st[1], env):
                r = self.block(st[2], env)
                if r is BRK:
                    break
                if r is not None and r is not CNT:
                    return r
        elif k == 'for':
            lst = self.ev(st[3], env)
            for i in range(len(lst)):
                sc = {st[2]: Cell(lst[i])}
                if st[1]:
                    sc[st[1]] = Cell(i)
                env.append(sc)
                try:
                    r = self.block(st[4], env)
                finally:
                    env.pop()
                if r is BRK:
                    break
                if r is not None and r is not CNT:
                    return r
        elif k == 'break':
            return BRK
        elif k == 'continue':
            return CNT
        elif k == 'match':
            v = self.ev(st[1], env)
            for pat, blk, _ in st[2]:
                sc = {}
                if pat[0] == 'v':
                    if not isinstance(v, EnumV) or v.name != pat[1]:
                        continue
                    for b, x in zip(pat[2], v.vals):
                        if b != '_':
                            sc[b] = Cell(x)
                elif pat[0] == 'lit':
                    if not same(v, self.ev(pat[1], env)):
                        continue
                elif pat[0] == 'bind':
                    sc[pat[1]] = Cell(v)
                env.append(sc)
                try:
                    return self.block(blk, env)
                finally:
                    env.pop()
            raise OracleError('no arm matches')
        elif k == 'return':
            return ('ret', self.cp(self.ev(st[1], env)) if st[1] is not None else None)
        elif k == 'eprint':
            self.err.append(self.ev(st[1], env) + b'\n')
        elif k == 'exit':
            raise ExitProg(self.ev(st[1], env) & 255)
        else:
            raise OracleError('statement ' + k)
        return None


# ------------------------------------------------------------ generator

class Retry(Exception):
    """The generator cannot build what it was asked for here."""


class Var:
    __slots__ = ('name', 't', 'mut', 'frozen', 'fixed', 'known', 'idx_of')

    def __init__(self, name, t, mut):
        self.name = name
        self.t = t
        self.mut = mut      # the generator may change it
        self.frozen = 0     # a `for` or `match` is looking at it in place
        self.fixed = 0      # its length is being relied on
        self.known = 0      # elements 0..known-1 exist
        self.idx_of = None  # the list variable this is a valid index of


class Sig:
    def __init__(self, name, params, ret, rec=False):
        self.name = name
        self.params = params
        self.ret = ret
        self.rec = rec
        self.has_mut = any(m for _, _, m in params)


EDGE_INTS = [0, 0, 1, 1, -1, 2, -2, 3, 5, 7, 10, 16, 31, 32, 63, 64, 65, 100, 127, 128, 255, 256, -128,
             1000, 65535, 65536, (1 << 31) - 1, 1 << 31, -(1 << 31), (1 << 32) - 1, 1 << 32, 1 << 62,
             MAX, MAX - 1, MIN, MIN + 1, 0x5555555555555555, -0x5555555555555556, 1234567890123]
DIVISORS = [1, -1, 2, -2, 3, -3, 4, 5, 6, 7, 8, 9, 10, -10, 11, 12, 13, 16, 25, 60, 100, 127, 128, 255, 256, 641,
            1000, 1024, 65535, 65536, 1000000, 1000000007, (1 << 31) - 1, 1 << 31, 1 << 32, (1 << 32) + 1,
            1 << 62, MAX, MIN, MIN + 1, -(1 << 31), 0x5555555555555555]
SHIFTS = [0, 0, 1, 1, 2, 3, 7, 8, 31, 32, 33, 62, 63, 63, 64, 64, 65, 127, 128, -1, -64]
STRINGS = [b'', b'', b'a', b'ab', b'abc', b' ', b'hello world', b'x,y,,z', b',', b',,', b'a,b', b'line1\nline2',
           b'one\r\ntwo\r\n', b'\n', b'\t tab ', b'  pad  ', b'-42', b'123', b'0', b'-', b'-0', b'12a',
           b'9223372036854775807', b'9223372036854775808', b'-9223372036854775807', b'a"q', b'back\\slash',
           b'The quick brown fox', b'zz', b'aaa', b'aa', b'key=value', b'=', b'!', b'azoL']


class Gen:
    def __init__(self, seed):
        self.r = random.Random(seed)
        self.seed = seed
        self.prog = Program()
        self.prog.seed = seed
        self.d = self.prog.decls
        self.n = 0
        self.scopes = []
        self.fns = []
        self.makers = {}
        self.banned = ()
        self.loop = 0
        self.budget = 0
        self.ret = None
        self.cur = None
        self.stats = Counter()
        self.consts = []
        self.const_values = {}
        r = self.r
        self.maxd = r.choice([1, 2, 2, 2, 3, 3, 4])
        self.nest = r.choice([2, 2, 3, 3, 4, 5])
        # One program in five is mostly numbers in deep expressions.
        self.arith = r.random() < 0.2
        if self.arith:
            self.maxd = r.choice([4, 5, 6])
        self.weights = {
            'let': 22 * r.uniform(0.5, 1.5), 'print': 14, 'assign': 16 * r.uniform(0.5, 1.5),
            'if': 9, 'while': 5 * r.uniform(0.3, 1.7), 'for': 8 * r.uniform(0.3, 1.7),
            'match': 8 * r.uniform(0.3, 1.7), 'push': 7 * r.uniform(0.3, 1.7), 'call': 6 * r.uniform(0.3, 1.7),
            'ctl': 3, 'ret': 2, 'known': 3, 'show': 3, 'stop': 0.12, 'exit': 0.05, 'eprint': 0.3, 'grow': 1.2,
        }

    # ---- names and scopes

    def fresh(self, prefix='v'):
        self.n += 1
        return prefix + str(self.n)

    def visible(self):
        seen = set()
        out = []
        for sc in reversed(self.scopes):
            for name, v in sc.items():
                if name not in seen:
                    seen.add(name)
                    if v not in self.banned:
                        out.append(v)
        return out

    def find(self, name):
        for sc in reversed(self.scopes):
            if name in sc:
                return sc[name]
        return None

    def declare(self, name, t, mut):
        v = Var(name, t, mut)
        self.scopes[-1][name] = v
        return v

    def let_name(self):
        """A name for a new local; now and then one that hides an outer variable."""
        if len(self.scopes) >= 3 and self.r.random() < 0.12:
            outer = [n for sc in self.scopes[:-1] for n in sc if n not in self.scopes[-1]]
            if outer:
                self.stats['shadowing'] += 1
                return self.r.choice(outer)
        return self.fresh()

    def vars_of(self, t):
        return [v for v in self.visible() if v.t == t]

    def root(self, e):
        """The variable a place expression is part of, or None for a value."""
        while e.k in ('field', 'index'):
            e = e.a[0]
        return self.find(e.a[0]) if e.k == 'var' else None

    def use(self, v):
        return E('var', v.t, v.name)

    # ---- declarations

    def rtype(self, d):
        r = self.r
        x = r.random()
        if d <= 0 or x < 0.45:
            return r.choice(self.leaf_types)
        k = r.choice(['list', 'list', 'list', 'opt', 'opt', 'res', 'box', 'pair', 'either'])
        if k == 'list':
            return TList(self.rtype(d - 1))
        if k == 'opt':
            return TOpt(self.rtype(d - 1))
        if k == 'res':
            return TRes(self.rtype(d - 1), self.rtype(0))
        if k == 'box':
            return TStruct('Box', self.rtype(d - 1))
        if k == 'pair':
            return TStruct('Pair', self.rtype(d - 1), self.rtype(d - 1))
        return TEnum('Either', self.rtype(d - 1), self.rtype(0))

    def declarations(self):
        r = self.r
        d = self.d
        d.add_struct('Box', ['T'], [('item', TP('T')), ('count', I64)])
        d.add_struct('Pair', ['A', 'B'], [('first', TP('A')), ('second', TP('B'))])
        d.add_enum('Either', ['A', 'B'], [('Left', [TP('A')]), ('Right', [TP('B')])])
        self.leaf_types = [I64, I64, I64, I64, STR, STR, BOOL]
        unit = TEnum('Color')
        d.add_enum('Color', [], [(v, []) for v in ['Red', 'Green', 'Blue', 'Black'][:r.randint(2, 4)]])
        self.unit = unit
        self.leaf_types.append(unit)
        vn = 0
        for i in range(r.randint(2, 5)):
            if r.random() < 0.6:
                name = 'S' + str(i)
                fields = [('f' + str(j), self.rtype(r.choice([0, 0, 1, 1, 2]))) for j in range(r.randint(1, 5))]
                d.add_struct(name, [], fields)
                self.leaf_types.append(TStruct(name))
            else:
                name = 'E' + str(i)
                variants = []
                for _ in range(r.randint(1, 4)):
                    vn += 1
                    variants.append(('V' + str(vn), [self.rtype(r.choice([0, 0, 1])) for _ in range(r.choice([0, 1, 1, 2, 3]))]))
                d.add_enum(name, [], variants)
                self.leaf_types.append(TEnum(name))
        if r.random() < 0.3:
            d.add_enum('Tree', [], [('Leaf', [I64]), ('Node', [TList(TEnum('Tree'))])])
            self.leaf_types.append(TEnum('Tree'))
        pool = [I64, I64, I64, BOOL, STR, STR, TList(I64), TList(STR), unit]
        for _ in range(r.randint(5, 10)):
            pool.append(self.rtype(r.choice([0, 1, 1, 2, 2, 3])))
        self.pool = pool
        # Every type a program can meet, to know which need a maker function.
        closure = []

        def walk(t):
            if t in closure or t[0] == 'tp':
                return
            closure.append(t)
            if t[0] == 'list':
                walk(t[1])
            elif t[0] == 'struct':
                for x in t[2]:
                    walk(x)
                for _, ft in d.fields(t):
                    walk(ft)
            elif t[0] == 'enum':
                for x in t[2]:
                    walk(x)
                for _, ps in d.variants(t):
                    for p in ps:
                        walk(p)
        for t in pool:
            walk(t)
        self.closure = closure

    def constants(self):
        r = self.r
        for i in range(r.randint(0, 5)):
            name = 'C' + str(i)
            k = r.choice(['i', 'i', 'i', 'b', 's'])
            if k == 'i':
                e = self.const_expr(r.choice([0, 1, 2, 3]))
                try:
                    ip = Interp(self.prog)
                    ip.consts = dict(self.const_values)
                    ip.ev(e, [])
                except Stop:
                    e = self.int_lit()
                t = I64
            elif k == 'b':
                e = E('bin', BOOL, r.choice(CMP), self.const_expr(1), self.const_expr(1))
                if r.random() < 0.3:
                    e = E('not', BOOL, e)
                t = BOOL
            else:
                e = E('str', STR, r.choice(STRINGS))
                t = STR
            self.prog.consts.append((name, t, e))
            self.consts.append((name, t))
            ip = Interp(self.prog)
            for cn, _, ce in self.prog.consts:
                ip.consts[cn] = ip.ev(ce, [])
            self.const_values = dict(ip.consts)

    def const_expr(self, d):
        r = self.r
        ints = [n for n, t in self.consts if t == I64]
        if d <= 0:
            if ints and r.random() < 0.3:
                return E('const', I64, r.choice(ints))
            return self.int_lit()
        op = r.choice(['+', '-', '*', '/', '%', '<<', '>>', '&', '|', '^', 'neg'])
        if op == 'neg':
            return E('neg', I64, self.const_expr(d - 1))
        if op in ('/', '%'):
            return E('bin', I64, op, self.const_expr(d - 1), E('int', I64, r.choice(DIVISORS), 'd'))
        if op in ('<<', '>>'):
            return E('bin', I64, op, self.const_expr(d - 1), E('int', I64, r.choice(SHIFTS), 'd'))
        return E('bin', I64, op, self.const_expr(d - 1), self.const_expr(d - 1))

    # ---- literals

    def int_lit(self, small=False):
        r = self.r
        x = r.random()
        if small or x < 0.45:
            v = r.randint(0, 20)
        elif x < 0.9:
            v = r.choice(EDGE_INTS)
        elif x < 0.95:
            v = r.randint(-1000, 1000)
        else:
            v = wrap(r.getrandbits(64))
        return E('int', I64, v, r.choice(['d', 'd', 'd', 'd', 'x', 'b', 'u', 'c']))

    def lit(self, v):
        return E('int', I64, v, 'd')

    def str_lit(self):
        r = self.r
        s = r.choice(STRINGS)
        if r.random() < 0.15:
            s = s + r.choice(STRINGS)
        if r.random() < 0.02:
            s = s + b'\0z'
        return E('str', STR, s)

    def selfable(self, t, seen=()):
        """Can a value of this type be written where no type is expected?"""
        k = t[0]
        if k == 'list':
            return self.selfable(t[1])
        if k == 'struct':
            return all(self.selfable(x) for x in t[2])
        if k == 'enum':
            if t[1] == 'Option':
                return self.selfable(t[2][0])
            if t[2]:
                return t in self.makers
        return True

    def base_variant(self, t):
        for v, ps in self.d.variants(t):
            if not any(self.mentions(p, t[1]) for p in ps):
                return v, ps
        raise Retry()

    def mentions(self, t, name):
        if t[0] == 'list':
            return self.mentions(t[1], name)
        if t[0] in ('struct', 'enum'):
            return t[1] == name or any(self.mentions(x, name) for x in t[2])
        return False

    def leaf(self, t, ctx):
        r = self.r
        k = t[0]
        if k == 'i64':
            return self.int_lit()
        if k == 'bool':
            return E('bool', BOOL, r.random() < 0.5)
        if k == 'str':
            return self.str_lit()
        if k == 'list':
            if ctx and r.random() < 0.25:
                return E('llit', t, [])
            return E('llit', t, [self.expr(t[1], 0, ctx) for _ in range(r.choice([1, 1, 2, 3]))])
        if k == 'struct':
            inner = not t[2]
            return E('slit', t, t[1], [(f, self.expr(ft, 0, inner)) for f, ft in self.d.fields(t)])
        if k == 'enum':
            generic = bool(t[2])
            if generic and not ctx:
                if t[1] == 'Option':
                    return E('ctor', t, 'Some', [self.expr(t[2][0], 0, False)])
                if t in self.makers:
                    return E('call', t, self.makers[t], [self.int_lit(True)])
                raise Retry()
            if t[1] == 'Tree':
                v, ps = self.base_variant(t)
            else:
                v, ps = r.choice(self.d.variants(t))
            return E('ctor', t, v, [self.expr(p, 0, not generic) for p in ps])
        raise Retry()

    # ---- expressions

    def paths(self, t):
        """Parts of visible variables that have type `t`."""
        out = []
        for v in self.visible():
            self.walk_paths(self.use(v), v, t, 2, out)
        return out

    def walk_paths(self, base, v, t, depth, out):
        bt = base.t
        if bt[0] == 'struct':
            for f, ft in self.d.fields(bt):
                e = E('field', ft, base, f)
                if ft == t:
                    out.append(e)
                if depth > 0 and ft[0] in ('struct',):
                    self.walk_paths(e, v, t, depth - 1, out)
        elif bt[0] == 'list' and base.k == 'var':
            idx = [self.lit(i) for i in range(min(v.known, 4))]
            for w in self.visible():
                if w.idx_of is v:
                    idx.append(self.use(w))
            if idx:
                e = E('index', bt[1], base, self.r.choice(idx))
                if bt[1] == t:
                    out.append(e)
                if depth > 0 and bt[1][0] == 'struct':
                    self.walk_paths(e, v, t, depth - 1, out)

    def call(self, sig, d):
        args = []
        for i, (_, pt, _) in enumerate(sig.params):
            if sig.rec and i == 0:
                args.append(self.lit(self.r.randint(0, 4)))
            else:
                args.append(self.expr(pt, d - 1, True))
        self.stats['call'] += 1
        return E('call', sig.ret, sig.name, args)

    def expr(self, t, d, ctx=True):
        r = self.r
        vs = self.vars_of(t)
        if d <= 0:
            if vs and r.random() < 0.75:
                return self.use(r.choice(vs))
            if r.random() < 0.3:
                ps = self.paths(t)
                if ps:
                    return r.choice(ps)
            if t in (I64, BOOL, STR) and r.random() < 0.15:
                cs = [n for n, ct in self.consts if ct == t]
                if cs:
                    self.stats['constant use'] += 1
                    return E('const', t, r.choice(cs))
            return self.leaf(t, ctx)
        x = r.random()
        if self.arith and t in (I64, BOOL) and x >= 0.22 and r.random() < 0.8:
            return self.typed(t, d, ctx)
        if vs and x < 0.22:
            return self.use(r.choice(vs))
        if x < 0.30:
            ps = self.paths(t)
            if ps:
                return r.choice(ps)
        if x < 0.38:
            fs = [f for f in self.fns if f.ret == t and not f.has_mut]
            if fs:
                return self.call(r.choice(fs), d)
        if x < 0.44 and self.selfable(t):
            return self.generic(t, d)
        return self.typed(t, d, ctx)

    def generic(self, t, d):
        r = self.r
        k = r.choice(['pick', 'pick', 'at_or', 'last_or', 'unwrap_or'])
        self.stats['generic call ' + k] += 1
        if k == 'pick':
            return E('call', t, 'pick', [self.expr(t, d - 1, False), self.expr(t, d - 1, False), self.expr(BOOL, d - 1, False)])
        if k == 'at_or':
            return E('call', t, 'at_or', [self.expr(TList(t), d - 1, False), self.expr(I64, d - 1, False), self.expr(t, d - 1, False)])
        if k == 'last_or':
            return E('call', t, 'last_or', [self.expr(TList(t), d - 1, False), self.expr(t, d - 1, False)])
        return E('call', t, 'unwrap_or', [self.expr(TOpt(t), d - 1, False), self.expr(t, d - 1, False)])

    def some_list_type(self):
        ls = [t for t in self.closure if t[0] == 'list' and self.selfable(t)]
        return self.r.choice(ls) if ls else TList(I64)

    def typed(self, t, d, ctx):
        r = self.r
        k = t[0]
        st = self.stats
        if k == 'i64':
            c = r.choice(['arith'] * 5 + ['div'] * 2 + ['shift'] * 2 + ['bit'] * 2 + ['neg', 'len', 'len', 'byte', 'lit', 'count', 'unwrap'])
            if c == 'arith':
                st['+ - *'] += 1
                return E('bin', I64, r.choice(['+', '+', '-', '-', '*']), self.expr(I64, d - 1, False), self.expr(I64, d - 1, False))
            if c == 'div':
                x = r.random()
                if x < 0.72:
                    st['/ % by a constant'] += 1
                    rhs = E('int', I64, r.choice(DIVISORS), 'd')
                elif x < 0.97:
                    st['/ % by a value'] += 1
                    rhs = E('bin', I64, '|', self.expr(I64, d - 1, False), self.lit(1))
                else:
                    st['/ % unguarded'] += 1
                    rhs = self.expr(I64, d - 1, False)
                return E('bin', I64, r.choice(['/', '%']), self.expr(I64, d - 1, False), rhs)
            if c == 'shift':
                st['<< >>'] += 1
                rhs = E('int', I64, r.choice(SHIFTS), 'd') if r.random() < 0.6 else self.expr(I64, d - 1, False)
                return E('bin', I64, r.choice(['<<', '>>']), self.expr(I64, d - 1, False), rhs)
            if c == 'bit':
                st['& | ^'] += 1
                return E('bin', I64, r.choice(['&', '|', '^']), self.expr(I64, d - 1, False), self.expr(I64, d - 1, False))
            if c == 'neg':
                st['unary -'] += 1
                return E('neg', I64, self.expr(I64, d - 1, False))
            if c == 'len':
                st['len'] += 1
                rt = STR if r.random() < 0.5 else self.some_list_type()
                return E('m', I64, 'len', self.expr(rt, d - 1, False), [])
            if c == 'byte':
                st['string byte'] += 1
                return E('call', I64, 'byte_at', [self.expr(STR, d - 1, True), self.expr(I64, d - 1, True)])
            if c == 'count':
                st['generic call count'] += 1
                return E('call', I64, 'count', [self.expr(self.some_list_type(), d - 1, False)])
            if c == 'unwrap':
                return E('call', I64, 'unwrap_or', [self.typed(TOpt(I64), d, False), self.expr(I64, d - 1, False)])
            return self.int_lit()
        if k == 'bool':
            c = r.choice(['cmp'] * 5 + ['streq', 'booleq', 'enumeq', 'and', 'and', 'or', 'or', 'not', 'text', 'has', 'lit'])
            if c == 'cmp':
                st['comparison'] += 1
                return E('bin', BOOL, r.choice(CMP), self.expr(I64, d - 1, False), self.expr(I64, d - 1, False))
            if c == 'streq':
                st['string =='] += 1
                return E('bin', BOOL, r.choice(['==', '!=']), self.expr(STR, d - 1, False), self.expr(STR, d - 1, False))
            if c == 'booleq':
                return E('bin', BOOL, r.choice(['==', '!=']), self.expr(BOOL, d - 1, False), self.expr(BOOL, d - 1, False))
            if c == 'enumeq':
                st['enum =='] += 1
                return E('bin', BOOL, r.choice(['==', '!=']), self.expr(self.unit, d - 1, False), self.expr(self.unit, d - 1, False))
            if c == 'and':
                st['&&'] += 1
                return E('and', BOOL, self.expr(BOOL, d - 1, False), self.expr(BOOL, d - 1, False))
            if c == 'or':
                st['||'] += 1
                return E('or', BOOL, self.expr(BOOL, d - 1, False), self.expr(BOOL, d - 1, False))
            if c == 'not':
                return E('not', BOOL, self.expr(BOOL, d - 1, False))
            if c == 'text':
                m = r.choice(['contains', 'starts_with', 'ends_with'])
                st['str.' + m] += 1
                return E('m', BOOL, m, self.expr(STR, d - 1, False), [self.expr(STR, d - 1, False)])
            if c == 'has':
                et = r.choice([I64, STR, BOOL, self.unit])
                st['list.contains'] += 1
                return E('m', BOOL, 'contains', self.expr(TList(et), d - 1, False), [self.expr(et, d - 1, False)])
            return E('bool', BOOL, r.random() < 0.5)
        if k == 'str':
            c = r.choice(['cat'] * 4 + ['stri', 'stri', 'strb', 'chr', 'sub', 'sub', 'slice', 'trim', 'join', 'lit', 'lit'])
            if c == 'cat':
                st['string +'] += 1
                return E('bin', STR, '+', self.expr(STR, d - 1, False), self.expr(STR, d - 1, False))
            if c == 'stri':
                st['str()'] += 1
                return E('builtin', STR, 'str', [self.expr(I64, d - 1, False)])
            if c == 'strb':
                return E('builtin', STR, 'str', [self.expr(BOOL, d - 1, False)])
            if c == 'chr':
                st['chr()'] += 1
                mask = self.lit(127 if r.random() < 0.8 else 255)
                return E('builtin', STR, 'chr', [E('bin', I64, '&', self.expr(I64, d - 1, False), mask)])
            if c == 'sub':
                st['str.slice'] += 1
                return E('call', STR, 'sub', [self.expr(STR, d - 1, True), self.expr(I64, d - 1, True), self.expr(I64, d - 1, True)])
            if c == 'slice':
                vs = self.vars_of(STR)
                if vs:
                    s = self.use(r.choice(vs))
                    return E('m', STR, 'slice', s, [self.lit(0), E('m', I64, 'len', E('var', STR, s.a[0]), [])])
                return self.str_lit()
            if c == 'trim':
                st['str.trim'] += 1
                return E('m', STR, 'trim', self.expr(STR, d - 1, False), [])
            if c == 'join':
                st['list.join'] += 1
                return E('m', STR, 'join', self.expr(TList(STR), d - 1, False), [self.expr(STR, d - 1, False)])
            return self.str_lit()
        if k == 'list':
            el = t[1]
            c = r.choice(['lit', 'lit', 'lit', 'rev', 'split', 'rep'])
            if c == 'rev' and self.selfable(t):
                st['list.reversed'] += 1
                return E('m', t, 'reversed', self.expr(t, d - 1, False), [])
            if c == 'split' and el == STR:
                if r.random() < 0.3:
                    st['str.lines'] += 1
                    return E('m', t, 'lines', self.expr(STR, d - 1, False), [])
                st['str.split'] += 1
                return E('m', t, 'split', self.expr(STR, d - 1, False), [self.expr(STR, 0, False)])
            if c == 'rep' and self.selfable(el):
                st['generic call rep'] += 1
                return E('call', t, 'rep', [self.expr(el, d - 1, False), self.expr(I64, d - 1, False)])
            if ctx and r.random() < 0.12:
                return E('llit', t, [])
            st['list literal'] += 1
            return E('llit', t, [self.expr(el, d - 1, ctx) for _ in range(r.choice([1, 1, 2, 2, 3, 4]))])
        if k == 'struct':
            if t[1] == 'Box' and r.random() < 0.3 and self.selfable(t):
                st['generic call wrap'] += 1
                return E('call', t, 'wrap', [self.expr(t[2][0], d - 1, False), self.expr(I64, d - 1, False)])
            if t[1] == 'Pair' and r.random() < 0.25 and self.selfable(t):
                st['generic call swap'] += 1
                return E('call', t, 'swap', [self.expr(TStruct('Pair', t[2][1], t[2][0]), d - 1, False)])
            inner = not t[2]
            if not inner and not self.selfable(t):
                # A generic struct literal learns its types from its fields.
                return self.leaf(t, ctx) if False else self.struct_of_vars(t, d)
            st['struct literal'] += 1
            return E('slit', t, t[1], [(f, self.expr(ft, d - 1, inner)) for f, ft in self.d.fields(t)])
        if k == 'enum':
            generic = bool(t[2])
            if t[1] == 'Option' and r.random() < 0.35:
                el = t[2][0]
                if el == I64 and r.random() < 0.6:
                    if r.random() < 0.5:
                        st['str.find'] += 1
                        return E('m', t, 'find', self.expr(STR, d - 1, False), [self.expr(STR, d - 1, False)])
                    st['str.to_i64'] += 1
                    return E('m', t, 'to_i64', self.expr(STR, d - 1, False), [])
                if self.selfable(el):
                    st['list.first/last'] += 1
                    return E('m', t, r.choice(['first', 'last']), self.expr(TList(el), d - 1, False), [])
            if generic and not ctx:
                return self.leaf(t, False)
            st['enum value'] += 1
            v, ps = r.choice(self.d.variants(t))
            return E('ctor', t, v, [self.expr(p, d - 1, not generic) for p in ps])
        raise Retry()

    def struct_of_vars(self, t, d):
        return E('slit', t, t[1], [(f, self.expr(ft, d - 1, False)) for f, ft in self.d.fields(t)])

    def unctx(self, t, d):
        """An expression that needs no expected type."""
        if not self.selfable(t):
            vs = self.vars_of(t)
            if vs:
                return self.use(self.r.choice(vs))
            raise Retry()
        return self.expr(t, d, False)

    # ---- statements

    def pick_type(self):
        r = self.r
        if self.arith and r.random() < 0.85:
            return I64 if r.random() < 0.8 else BOOL
        if r.random() < 0.3:
            return I64
        return r.choice(self.pool)

    def show(self, e, depth=3):
        """Statements that print everything inside the place `e`."""
        t = e.t
        k = t[0]
        if k in ('i64', 'bool', 'str'):
            return [['print', e]]
        if depth <= 0:
            return [['print', E('str', STR, b'...')]]
        if k == 'list':
            it = self.fresh('it')
            body = self.show(E('var', t[1], it), depth - 1)
            return [['print', E('m', I64, 'len', e, [])], ['for', None, it, e, body]]
        if k == 'struct':
            out = []
            for f, ft in self.d.fields(t):
                out.extend(self.show(E('field', ft, copy.deepcopy(e), f), depth - 1))
            return out
        if k == 'enum':
            arms = []
            for v, ps in self.d.variants(t):
                names = [self.fresh('b') for _ in ps]
                body = [['print', E('str', STR, v.encode())]]
                for n, p in zip(names, ps):
                    body.extend(self.show(E('var', p, n), depth - 1))
                arms.append([('v', v, names), body, len(body) == 1])
            self.stats['match on an enum'] += 1
            return [['match', e, arms]]
        raise Retry()

    def block(self, n, d, tail=None):
        self.scopes.append({})
        out = []
        try:
            for _ in range(n):
                if self.budget <= 0:
                    break
                self.budget -= 1
                out.extend(self.safe_stmt(d))
            if tail:
                out.extend(tail())
        finally:
            self.scopes.pop()
        return out

    def safe_stmt(self, d):
        depth = len(self.scopes)
        loop = self.loop
        try:
            return self.stmt(d)
        except Retry:
            del self.scopes[depth:]
            self.loop = loop
            self.banned = ()
            return [['print', self.expr(I64, 0, False)]]

    def stmt(self, d):
        r = self.r
        w = self.weights
        kinds = list(w)
        k = r.choices(kinds, [w[x] for x in kinds])[0]
        if d <= 0 and k in ('if', 'while', 'for', 'match', 'known', 'grow'):
            k = r.choice(['let', 'print', 'assign', 'push'])
        self.stats['statement ' + k] += 1
        return getattr(self, 's_' + k)(d)

    def xd(self):
        return self.r.randint(0, self.maxd)

    def s_let(self, d):
        r = self.r
        t = self.pick_type()
        ann = r.random() < 0.5 or not self.selfable(t)
        e = self.expr(t, self.xd(), True) if ann else self.unctx(t, self.xd())
        if not ann and e.k in ('llit', 'ctor') and (e.k == 'llit' and not e.a[0] or e.k == 'ctor' and t[2] and e.a[0] != 'Some'):
            ann = True
        name = self.let_name()
        mut = r.random() < 0.65
        self.declare(name, t, mut)
        return [['let', name, mut, ann, t, e]]

    def s_print(self, d):
        r = self.r
        t = r.choice([I64, I64, I64, STR, STR, BOOL])
        return [['print', self.expr(t, self.xd(), False)]]

    def s_show(self, d):
        vs = [v for v in self.visible()]
        if not vs:
            return self.s_print(d)
        v = self.r.choice(vs)
        v.frozen += 1
        try:
            return self.show(self.use(v), 2)
        finally:
            v.frozen -= 1

    def s_eprint(self, d):
        return [['eprint', self.expr(STR, 1, False)]]

    def s_exit(self, d):
        return [['if', [[self.expr(BOOL, 1, False), [['exit', self.expr(I64, 1, False)]]]], None]]

    def s_stop(self, d):
        """A deliberate run-time stop, which the oracle predicts."""
        r = self.r
        c = r.choice(['div', 'rem', 'index', 'pop', 'chr', 'slice'])
        self.stats['deliberate stop'] += 1
        if c in ('div', 'rem'):
            x = self.expr(I64, 1, False)
            zero = E('bin', I64, '-', x, copy.deepcopy(x)) if x.k in ('var', 'int') else self.lit(0)
            st = ['print', E('bin', I64, '/' if c == 'div' else '%', self.expr(I64, 1, False), zero)]
        elif c == 'index':
            t = self.some_list_type()
            vs = [v for v in self.visible() if v.t[0] == 'list' and v.t[1] in (I64, STR, BOOL)]
            if not vs:
                raise Retry()
            v = r.choice(vs)
            st = ['print', E('index', v.t[1], self.use(v), E('m', I64, 'len', self.use(v), []))]
        elif c == 'pop':
            vs = [v for v in self.visible() if v.t[0] == 'list' and v.mut and not v.frozen and not v.fixed]
            if not vs:
                raise Retry()
            v = r.choice(vs)
            name = self.fresh()
            body = [['assign', E('var', I64, name), E('bin', I64, '+', E('var', I64, name), self.lit(1))],
                    ['expr', E('m', v.t[1], 'pop', self.use(v), [])]]
            return [['let', name, True, False, I64, self.lit(0)],
                    ['while', E('bin', BOOL, '<', E('var', I64, name), self.lit(5000)), body]]
        elif c == 'chr':
            st = ['print', E('builtin', STR, 'chr', [self.lit(r.choice([256, -1, 1000, MIN]))])]
        else:
            s = self.expr(STR, 0, False)
            st = ['print', E('m', STR, 'slice', s, [self.lit(r.choice([0, 1, 2, -1])), E('bin', I64, '+', E('m', I64, 'len', copy.deepcopy(s), []), self.lit(1))])]
        if r.random() < 0.5:
            return [st]
        return [['if', [[self.expr(BOOL, 1, False), [st]]], None]]

    def target(self, v):
        """A place inside the mutable variable `v`, and whether it needs a guard."""
        r = self.r
        e = self.use(v)
        guard = None
        whole = True
        indexed = False
        for _ in range(4):
            t = e.t
            if t[0] == 'struct' and r.random() < 0.65:
                f, ft = r.choice(self.d.fields(t))
                e = E('field', ft, e, f)
                whole = False
                continue
            if t[0] == 'list' and not indexed and r.random() < 0.5:
                indexed = True
                if whole and v.known and r.random() < 0.7:
                    idx = self.lit(r.randrange(v.known))
                else:
                    n = E('m', I64, 'len', copy.deepcopy(e), [])
                    guard = E('bin', BOOL, '>', n, self.lit(0))
                    idx = E('call', I64, 'clampi', [self.expr(I64, 1, True), copy.deepcopy(n)])
                e = E('index', t[1], e, idx)
                whole = False
                continue
            break
        if whole and v.fixed:
            raise Retry()
        return e, guard

    def s_assign(self, d):
        r = self.r
        cands = [v for v in self.visible() if v.mut and not v.frozen]
        if not cands:
            return self.s_let(d)
        v = r.choice(cands)
        svars = [x for x in cands if x.t == STR and not x.fixed]
        if svars and r.random() < 0.3:
            v = r.choice(svars)
            self.stats['string append'] += 1
            if r.random() < 0.2:
                self.stats['string self-append'] += 1
                body = [['assign', self.use(v), E('bin', STR, '+', self.use(v), self.use(v))]]
                return [['if', [[E('bin', BOOL, '<', E('m', I64, 'len', self.use(v), []), self.lit(200)), body]], None]]
            return [['assign', self.use(v), E('bin', STR, '+', self.use(v), self.expr(STR, self.xd(), False))]]
        ivars = [x for x in cands if x.t == I64 and not x.fixed]
        if ivars and r.random() < 0.25:
            v = r.choice(ivars)
            self.stats['i64 update'] += 1
            op = r.choice(['+', '-', '*', '^', '&', '|', '<<', '>>'])
            return [['assign', self.use(v), E('bin', I64, op, self.use(v), self.expr(I64, self.xd(), False))]]
        e, guard = self.target(v)
        rhs = self.expr(e.t, self.xd(), True)
        self.stats['assign to ' + e.k] += 1
        st = ['assign', e, rhs]
        if guard is not None:
            return [['if', [[guard, [st]]], None]]
        return [st]

    def s_if(self, d):
        r = self.r
        branches = []
        for _ in range(r.choice([1, 1, 1, 2, 3])):
            cond = self.expr(BOOL, self.xd(), False)
            branches.append([cond, self.block(r.randint(1, 3), d - 1)])
        els = self.block(r.randint(1, 3), d - 1) if r.random() < 0.5 else None
        return [['if', branches, els]]

    def s_while(self, d):
        r = self.r
        i = self.fresh('i')
        self.declare(i, I64, False)
        bound = self.lit(r.choice([0, 1, 2, 3, 3, 4, 6]))
        if r.random() < 0.3:
            ls = [v for v in self.visible() if v.t[0] == 'list']
            if ls:
                bound = E('m', I64, 'len', self.use(r.choice(ls)), [])
        cond = E('bin', BOOL, r.choice(['<', '<', '!=']) if bound.k == 'int' else '<', E('var', I64, i), bound)
        if r.random() < 0.25:
            self.stats['while with a compound condition'] += 1
            cond = E('and', BOOL, cond, self.expr(BOOL, self.xd(), False))
        step = ['assign', E('var', I64, i), E('bin', I64, '+', E('var', I64, i), self.lit(1))]
        self.loop += 1
        try:
            body = [step] + self.block(r.randint(1, 4), d - 1)
        finally:
            self.loop -= 1
        return [['let', i, True, False, I64, self.lit(0)], ['while', cond, body]]

    def s_grow(self, d):
        """A loop that makes a list or a string grow well past its first buffer."""
        r = self.r
        if self.loop > 0:
            return self.s_print(d)
        vs = [v for v in self.visible() if v.mut and not v.frozen and not v.fixed and (v.t == STR or v.t[0] == 'list')]
        if not vs:
            return self.s_let(d)
        v = r.choice(vs)
        i = self.fresh('i')
        self.declare(i, I64, False)
        iv = E('var', I64, i)
        step = ['assign', iv, E('bin', I64, '+', E('var', I64, i), self.lit(1))]
        self.scopes.append({})
        try:
            if v.t == STR:
                self.stats['string grown in a loop'] += 1
                piece = self.expr(STR, 1, False) if r.random() < 0.6 else E('builtin', STR, 'str', [E('var', I64, i)])
                grow = ['assign', self.use(v), E('bin', STR, '+', self.use(v), piece)]
                n = r.choice([20, 40, 70, 150, 300])
            else:
                self.stats['list grown in a loop'] += 1
                grow = ['expr', E('m', None, 'push', self.use(v), [self.expr(v.t[1], r.choice([0, 0, 1]), True)])]
                n = r.choice([9, 17, 33, 40, 70, 130])
        finally:
            self.scopes.pop()
        out = [['let', i, True, False, I64, self.lit(0)],
               ['while', E('bin', BOOL, '<', E('var', I64, i), self.lit(n)), [step, grow]],
               ['print', E('m', I64, 'len', self.use(v), [])]]
        if v.t[0] == 'list' and r.random() < 0.5:
            # ... and shrink again, so buffers are reused.
            j = self.fresh('i')
            self.declare(j, I64, False)
            out.append(['let', j, True, False, I64, self.lit(0)])
            out.append(['while', E('bin', BOOL, '<', E('var', I64, j), self.lit(n - r.randint(0, 5))), [
                ['assign', E('var', I64, j), E('bin', I64, '+', E('var', I64, j), self.lit(1))],
                ['expr', E('m', v.t[1], 'pop', self.use(v), [])]]])
        return out

    def s_ctl(self, d):
        if self.loop <= 0:
            return self.s_print(d)
        k = self.r.choice(['break', 'continue'])
        self.stats[k] += 1
        return [['if', [[self.expr(BOOL, self.xd(), False), [[k]]]], None]]

    def s_ret(self, d):
        if self.ret is None:
            body = [['return', None]]
        else:
            body = self.block(self.r.randint(0, 1), d - 1, lambda: [['return', self.expr(self.ret, self.xd(), True)]])
        self.stats['early return'] += 1
        return [['if', [[self.expr(BOOL, self.xd(), False), body]], None]]

    def s_for(self, d):
        r = self.r
        lvars = [v for v in self.visible() if v.t[0] == 'list']
        frozen = None
        x = r.random()
        if lvars and x < 0.5:
            frozen = r.choice(lvars)
            it = self.use(frozen)
            self.stats['for over a variable'] += 1
        else:
            it = None
            if x < 0.75:
                for v in r.sample(self.visible(), len(self.visible())):
                    out = []
                    self.list_fields(self.use(v), 2, out)
                    if out:
                        it = r.choice(out)
                        frozen = v
                        self.stats['for over a field'] += 1
                        break
            if it is None:
                it = self.unctx(self.some_list_type(), self.xd())
                frozen = self.root(it)
                self.stats['for over a temporary'] += 1
        item = self.fresh('it')
        index = self.fresh('ix') if r.random() < 0.5 else None
        if index:
            self.stats['for with an index'] += 1
        if frozen:
            frozen.frozen += 1
        self.loop += 1
        self.scopes.append({})
        try:
            self.declare(item, it.t[1], False)
            if index:
                iv = self.declare(index, I64, False)
                if it.k == 'var':
                    iv.idx_of = frozen
            body = self.block(r.randint(1, 4), d - 1)
        finally:
            self.scopes.pop()
            self.loop -= 1
            if frozen:
                frozen.frozen -= 1
        return [['for', index, item, it, body]]

    def list_fields(self, base, depth, out):
        if base.t[0] != 'struct':
            return
        for f, ft in self.d.fields(base.t):
            e = E('field', ft, base, f)
            if ft[0] == 'list':
                out.append(e)
            elif depth > 0:
                self.list_fields(e, depth - 1, out)

    def s_known(self, d):
        """`if xs.len() > k { ... }` with direct indexing inside."""
        r = self.r
        ls = [v for v in self.visible() if v.t[0] == 'list' and not v.known]
        if not ls:
            return self.s_let(d)
        v = r.choice(ls)
        k = r.choice([0, 0, 1, 2])
        self.stats['direct indexing'] += 1
        cond = E('bin', BOOL, '>', E('m', I64, 'len', self.use(v), []), self.lit(k))
        v.fixed += 1
        v.known = k + 1
        try:
            body = self.block(r.randint(1, 4), d - 1, lambda: [['print', E('m', I64, 'len', self.use(v), [])]] if self.find(v.name) is v else [])
        finally:
            v.fixed -= 1
            v.known = 0
        return [['if', [[cond, body]], None]]

    def s_match(self, d):
        r = self.r
        k = r.choice(['enum', 'enum', 'enum', 'int', 'int', 'bool', 'str'])
        frozen = None
        if k == 'enum':
            evars = [v for v in self.visible() if v.t[0] == 'enum']
            x = r.random()
            if evars and x < 0.55:
                frozen = r.choice(evars)
                sc = self.use(frozen)
                self.stats['match in place'] += 1
            else:
                ps = []
                if x < 0.75:
                    for v in self.visible():
                        self.enum_paths(self.use(v), v, 2, ps)
                if ps:
                    sc, frozen = r.choice(ps)
                    self.stats['match on a field'] += 1
                else:
                    ets = [t for t in self.closure if t[0] == 'enum' and self.selfable(t)]
                    if not ets:
                        raise Retry()
                    sc = self.unctx(r.choice(ets), self.xd())
                    frozen = self.root(sc)
                    self.stats['match on a temporary'] += 1
            variants = self.d.variants(sc.t)
            r.shuffle(variants)
            catch = len(variants) > 1 and r.random() < 0.3
            if catch:
                variants = variants[:r.randint(1, len(variants) - 1)]
            arms = []
            if frozen:
                frozen.frozen += 1
            try:
                for v, ps in variants:
                    names = [self.fresh('b') if r.random() < 0.8 else '_' for _ in ps]
                    arms.append([('v', v, names), self.arm(d, [(n, p) for n, p in zip(names, ps) if n != '_']), r.random() < 0.5])
                if catch:
                    arms.append(self.catch_arm(d, sc.t))
            finally:
                if frozen:
                    frozen.frozen -= 1
            self.stats['match on an enum'] += 1
            return [['match', sc, arms]]
        if k == 'int':
            sc = self.expr(I64, self.xd(), False)
            seen = set()
            arms = []
            for _ in range(r.randint(1, 4)):
                cs = [n for n, t in self.consts if t == I64]
                if cs and r.random() < 0.25:
                    n = r.choice(cs)
                    val = self.const_values[n]
                    pe = E('const', I64, n)
                else:
                    val = r.choice([0, 1, 2, 3, -1, 7, 10, 65, 97, MAX, 100, 255, 20, 4, 5, -2])
                    pe = E('int', I64, val, r.choice(['d', 'd', 'c', 'x']) if val >= 0 else 'd')
                if val in seen:
                    continue
                seen.add(val)
                arms.append([('lit', pe), self.arm(d, []), r.random() < 0.5])
            arms.append(self.catch_arm(d, I64))
            self.stats['match on a number'] += 1
            return [['match', sc, arms]]
        if k == 'bool':
            sc = self.expr(BOOL, self.xd(), False)
            first = r.random() < 0.5
            arms = [[('lit', E('bool', BOOL, first)), self.arm(d, []), r.random() < 0.5]]
            if r.random() < 0.6:
                arms.append([('lit', E('bool', BOOL, not first)), self.arm(d, []), r.random() < 0.5])
            else:
                arms.append(self.catch_arm(d, BOOL))
            self.stats['match on a bool'] += 1
            return [['match', sc, arms]]
        sc = self.expr(STR, self.xd(), False)
        seen = set()
        arms = []
        # A string variable is matched in place, so the arms must not change it.
        held = self.root(sc)
        if held:
            held.frozen += 1
        try:
            return [['match', sc, self.str_arms(d)]]
        finally:
            if held:
                held.frozen -= 1

    def str_arms(self, d):
        r = self.r
        seen = set()
        arms = []
        for _ in range(r.randint(1, 3)):
            cs = [n for n, t in self.consts if t == STR]
            if cs and r.random() < 0.25:
                n = r.choice(cs)
                val = self.const_values[n]
                pe = E('const', STR, n)
            else:
                val = r.choice(STRINGS)
                pe = E('str', STR, val)
            if val in seen:
                continue
            seen.add(val)
            arms.append([('lit', pe), self.arm(d, []), r.random() < 0.5])
        arms.append(self.catch_arm(d, STR))
        self.stats['match on a string'] += 1
        return arms

    def enum_paths(self, base, v, depth, out):
        if base.t[0] != 'struct':
            return
        for f, ft in self.d.fields(base.t):
            e = E('field', ft, base, f)
            if ft[0] == 'enum':
                out.append((e, v))
            elif depth > 0:
                self.enum_paths(e, v, depth - 1, out)

    def arm(self, d, bindings):
        self.scopes.append({})
        try:
            for n, t in bindings:
                self.declare(n, t, False)
            return self.block(self.r.choice([1, 1, 1, 2, 3]), d - 1)
        finally:
            self.scopes.pop()

    def catch_arm(self, d, t):
        r = self.r
        if r.random() < 0.5:
            return [('_',), self.arm(d, []), r.random() < 0.5]
        n = self.fresh('b')
        self.stats['binding arm'] += 1
        return [('bind', n), self.arm(d, [(n, t)]), r.random() < 0.5]

    def s_push(self, d):
        r = self.r
        recvs = []
        for v in self.visible():
            if not v.mut or v.frozen:
                continue
            if v.t[0] == 'list' and not v.fixed:
                recvs.append(self.use(v))
            out = []
            self.list_fields(self.use(v), 2, out)
            recvs.extend(out)
        if not recvs:
            return self.s_let(d)
        recv = r.choice(recvs)
        el = recv.t[1]
        x = r.random()
        if x < 0.72:
            self.stats['push'] += 1
            return [['expr', E('m', None, 'push', recv, [self.expr(el, self.xd(), True)])]]
        self.stats['pop'] += 1
        pop = E('m', el, 'pop', recv, [])
        if x < 0.74:
            return [['expr', pop]]
        name = self.fresh()
        body = [['let', name, False, r.random() < 0.5, el, pop]] + self.show(E('var', el, name), 2)
        guard = E('bin', BOOL, '>', E('m', I64, 'len', copy.deepcopy(recv), []), self.lit(0))
        return [['if', [[guard, body]], None]]

    def mut_args(self, sig, d, first=None):
        """Arguments for a call, with different variables for the `mut` ones."""
        r = self.r
        chosen = {}
        used = []
        for i, (_, pt, m) in enumerate(sig.params):
            if not m:
                continue
            cands = [v for v in self.visible() if v.t == pt and v.mut and not v.frozen and not v.fixed and v not in used]
            places = [(self.use(v), v) for v in cands]
            for v in self.visible():
                if v.mut and not v.frozen and v not in used and v.t[0] == 'struct':
                    for f, ft in self.d.fields(v.t):
                        if ft == pt:
                            places.append((E('field', ft, self.use(v), f), v))
                if v.mut and not v.frozen and v not in used and v.known and v.t == TList(pt):
                    places.append((E('index', pt, self.use(v), self.lit(r.randrange(v.known))), v))
            if not places:
                raise Retry()
            e, v = r.choice(places)
            used.append(v)
            chosen[i] = E('mut', pt, e)
        old = self.banned
        self.banned = tuple(used)
        try:
            args = []
            for i, (_, pt, m) in enumerate(sig.params):
                if m:
                    args.append(chosen[i])
                elif i == 0 and first is not None:
                    args.append(first)
                elif i == 0 and sig.rec:
                    args.append(self.lit(r.randint(0, 4)))
                else:
                    args.append(self.expr(pt, min(d, 1), True))
        finally:
            self.banned = old
        return args

    def s_call(self, d):
        r = self.r
        x = r.random()
        if x < 0.2:
            ls = [v for v in self.visible() if v.t[0] == 'list' and v.mut and not v.frozen and not v.fixed and self.selfable(v.t[1])]
            if ls:
                v = r.choice(ls)
                old = self.banned
                self.banned = (v,)
                try:
                    arg = self.expr(v.t[1], 1, False)
                finally:
                    self.banned = old
                self.stats['generic call twice'] += 1
                return [['expr', E('call', None, 'twice', [E('mut', v.t, self.use(v)), arg])]]
        fs = [f for f in self.fns if f.has_mut or f.ret is None]
        if not fs:
            fs = list(self.fns)
        if not fs:
            return self.s_let(d)
        sig = r.choice(fs)
        args = self.mut_args(sig, d)
        call = E('call', sig.ret, sig.name, args)
        if sig.has_mut:
            self.stats['call with mut'] += 1
        if sig.ret is None or r.random() < 0.2:
            return [['expr', call]]
        name = self.fresh()
        self.declare(name, sig.ret, False)
        return [['let', name, False, False, sig.ret, call]] + (self.show(E('var', sig.ret, name), 2) if r.random() < 0.6 else [])

    # ---- functions

    def function(self, name, ret='pick', params=None, size=None):
        r = self.r
        if ret == 'pick':
            ret = None if r.random() < 0.12 else self.pick_type()
        rec = False
        if params is None:
            params = []
            for _ in range(r.choice([0, 1, 1, 2, 2, 3, 3, 4, 5, 6, 8])):
                params.append((self.fresh('p'), self.pick_type(), r.random() < 0.25))
            rec = r.random() < 0.25
            if rec:
                params = [(self.fresh('depth'), I64, False)] + params[:7]
        sig = Sig(name, params, ret, rec)
        self.ret = ret
        self.loop = 0
        self.budget = size if size is not None else r.randint(3, 14)
        self.scopes = [{}]
        for pn, pt, pm in params:
            self.declare(pn, pt, pm)
        self.scopes.append({})
        body = []
        n = self.budget
        rec_at = r.randrange(n) if rec else -1
        for i in range(n):
            if i == rec_at:
                try:
                    body.extend(self.recursion(sig))
                except Retry:
                    pass
            if self.budget <= 0:
                break
            self.budget -= 1
            body.extend(self.safe_stmt(self.nest))
        if r.random() < 0.3:
            for pn, pt, pm in params:
                if pm:
                    body.extend(self.show(E('var', pt, pn), 2))
        if ret is not None:
            body.append(['return', self.expr(ret, self.xd(), True)])
        self.scopes = []
        f = Fn(name, [], params, ret, body)
        f.rec = rec
        self.prog.fns.append(f)
        self.fns.append(sig)
        self.stats['function with %d parameters' % len(params)] += 1
        if rec:
            self.stats['recursive function'] += 1
        return sig

    def recursion(self, sig):
        depth = sig.params[0][0]
        down = E('bin', I64, '-', E('var', I64, depth), self.lit(1))
        self.scopes.append({})
        try:
            args = self.mut_args(sig, 1, first=down)
            call = E('call', sig.ret, sig.name, args)
            if sig.ret is None:
                body = [['expr', call]]
            else:
                name = self.fresh()
                body = [['let', name, False, False, sig.ret, call]] + self.show(E('var', sig.ret, name), 2)
        finally:
            self.scopes.pop()
        return [['if', [[E('bin', BOOL, '>', E('var', I64, depth), self.lit(0)), body]], None]]

    def big_frame(self):
        """A function with hundreds of locals, so frame offsets pass 4 KB."""
        r = self.r
        n = r.randint(300, 900)
        seed = 'seed'
        body = []
        prev = seed
        names = []
        for i in range(n):
            name = 'w' + str(i)
            op = r.choice(['+', '-', '*', '^'])
            body.append(['let', name, i % 7 == 0, False, I64, E('bin', I64, op, E('var', I64, prev), self.lit(r.randint(1, 99)))])
            names.append(name)
            prev = name if r.random() < 0.8 else r.choice(names)
        for i in range(0, n, 7):
            other = r.choice(names)
            body.append(['assign', E('var', I64, names[i]), E('bin', I64, '+', E('var', I64, names[i]), E('var', I64, other))])
        total = E('var', I64, names[-1])
        for name in r.sample(names, 12):
            total = E('bin', I64, r.choice(['+', '^', '-']), total, E('var', I64, name))
            body.append(['print', E('var', I64, name)])
        body.append(['return', total])
        self.prog.fns.append(Fn('big_frame', [], [(seed, I64, False)], I64, body))
        self.fns.append(Sig('big_frame', [(seed, I64, False)], I64))
        self.stats['large frame'] += 1

    def helpers(self):
        """The hand-written helper functions every program starts with."""
        T = TP('T')
        v = lambda n, t=I64: E('var', t, n)
        i = self.lit
        ln = lambda e: E('m', I64, 'len', e, [])
        b = lambda op, l, r, t=I64: E('bin', t, op, l, r)
        fns = [
            Fn('clampi', [], [('e', I64, False), ('n', I64, False)], I64, [
                ['if', [[b('<=', v('n'), i(0), BOOL), [['return', i(0)]]]], None],
                ['let', 'm', False, False, I64, b('%', v('e'), v('n'))],
                ['if', [[b('<', v('m'), i(0), BOOL), [['return', b('+', v('m'), v('n'))]]]], None],
                ['return', v('m')]]),
            Fn('byte_at', [], [('s', STR, False), ('k', I64, False)], I64, [
                ['if', [[b('==', ln(v('s', STR)), i(0), BOOL), [['return', i(-1)]]]], None],
                ['return', E('index', I64, v('s', STR), E('call', I64, 'clampi', [v('k'), ln(v('s', STR))]))]]),
            Fn('sub', [], [('s', STR, False), ('a', I64, False), ('b', I64, False)], STR, [
                ['let', 'n', False, False, I64, b('+', ln(v('s', STR)), i(1))],
                ['let', 'lo', False, False, I64, E('call', I64, 'clampi', [v('a'), v('n')])],
                ['let', 'hi', False, False, I64, E('call', I64, 'clampi', [v('b'), v('n')])],
                ['if', [[b('<=', v('lo'), v('hi'), BOOL), [['return', E('m', STR, 'slice', v('s', STR), [v('lo'), v('hi')])]]]], None],
                ['return', E('m', STR, 'slice', v('s', STR), [v('hi'), v('lo')])]]),
            Fn('pick', ['T'], [('a', T, False), ('b', T, False), ('first', BOOL, False)], T, [
                ['if', [[v('first', BOOL), [['return', v('a', T)]]]], None],
                ['return', v('b', T)]]),
            Fn('at_or', ['T'], [('xs', TList(T), False), ('k', I64, False), ('d', T, False)], T, [
                ['if', [[b('==', ln(v('xs', TList(T))), i(0), BOOL), [['return', v('d', T)]]]], None],
                ['return', E('index', T, v('xs', TList(T)), E('call', I64, 'clampi', [v('k'), ln(v('xs', TList(T)))]))]]),
            Fn('count', ['T'], [('xs', TList(T), False)], I64, [
                ['let', 'n', True, False, I64, i(0)],
                ['for', None, 'x', v('xs', TList(T)), [['assign', v('n'), b('+', v('n'), i(1))]]],
                ['return', v('n')]]),
            Fn('wrap', ['T'], [('x', T, False), ('n', I64, False)], TStruct('Box', T), [
                ['return', E('slit', TStruct('Box', T), 'Box', [('item', v('x', T)), ('count', v('n'))])]]),
            Fn('swap', ['A', 'B'], [('p', TStruct('Pair', TP('A'), TP('B')), False)], TStruct('Pair', TP('B'), TP('A')), [
                ['return', E('slit', TStruct('Pair', TP('B'), TP('A')), 'Pair', [
                    ('first', E('field', TP('B'), v('p', None), 'second')), ('second', E('field', TP('A'), v('p', None), 'first'))])]]),
            Fn('rep', ['T'], [('x', T, False), ('n', I64, False)], TList(T), [
                ['let', 'out', True, True, TList(T), E('llit', TList(T), [])],
                ['let', 'k', True, False, I64, i(0)],
                ['while', b('<', v('k'), b('&', v('n'), i(3)), BOOL), [
                    ['expr', E('m', None, 'push', v('out', TList(T)), [v('x', T)])],
                    ['assign', v('k'), b('+', v('k'), i(1))]]],
                ['return', v('out', TList(T))]]),
            Fn('last_or', ['T'], [('xs', TList(T), False), ('d', T, False)], T, [
                ['match', E('m', TOpt(T), 'last', v('xs', TList(T)), []), [
                    [('v', 'Some', ['found']), [['return', v('found', T)]], True],
                    [('v', 'None', []), [['return', v('d', T)]], True]]]]),
            Fn('twice', ['T'], [('xs', TList(T), True), ('x', T, False)], None, [
                ['expr', E('m', None, 'push', v('xs', TList(T)), [v('x', T)])],
                ['expr', E('m', None, 'push', v('xs', TList(T)), [v('x', T)])]]),
            Fn('unwrap_or', ['T'], [('o', TOpt(T), False), ('d', T, False)], T, [
                ['match', v('o', TOpt(T)), [
                    [('v', 'Some', ['found']), [['return', v('found', T)]], True],
                    [('v', 'None', []), [['return', v('d', T)]], False]]]]),
        ]
        for f in fns:
            f.fixed = True
            self.prog.fns.append(f)

    def program(self):
        r = self.r
        self.declarations()
        self.constants()
        self.helpers()
        # Makers: a function for every generic enum type, so a value of it can
        # be written where the compiler has no expected type.
        need = [t for t in self.closure if t[0] == 'enum' and t[2] and t[1] != 'Option']
        for t in reversed(need):
            name = self.fresh('make')
            self.function(name, ret=t, params=[(self.fresh('p'), I64, False)], size=r.randint(0, 3))
            self.makers[t] = name
        if r.random() < 0.08:
            self.big_frame()
        for _ in range(r.randint(2, 6)):
            self.function(self.fresh('f'))
        # main: many live variables first, then statements, then everything shown.
        self.ret = I64
        self.loop = 0
        self.scopes = [{}]
        body = []
        for _ in range(r.randint(10, 18)):
            try:
                t = I64 if r.random() < 0.45 else r.choice(self.pool)
                name = self.fresh()
                e = self.expr(t, r.randint(0, 2), True)
                mut = r.random() < 0.75
                self.declare(name, t, mut)
                body.append(['let', name, mut, True, t, e])
            except Retry:
                pass
        self.budget = r.randint(15, 45)
        while self.budget > 0:
            self.budget -= 1
            body.extend(self.safe_stmt(self.nest))
        for v in list(self.scopes[0].values()):
            body.extend(self.show(self.use(v), 3))
        body.append(['return', self.expr(I64, 1, True)])
        self.scopes = []
        self.prog.fns.append(Fn('main', [], [], I64, body))
        return self.prog


def generate(seed):
    g = Gen(seed)
    return g.program(), g.stats


# --------------------------------------------------------------- running

EMU = None


def load_emulator():
    """The checking emulator for arm64 builds, when this machine has it."""
    global EMU
    where = os.environ.get('AZOL_EMU_DIR', '/home/claude/ref')
    if os.path.exists(os.path.join(where, 'emu.py')):
        sys.path.insert(0, where)
        try:
            import emu
            EMU = emu
        except Exception as x:   # the emulator needs packages that may be missing
            print('note: the arm64 emulator could not be loaded:', x)
    return EMU is not None


class Runner:
    def __init__(self, azol, arm=True, limit=120000):
        self.azol = azol
        self.arm = arm and EMU is not None
        self.limit = limit
        self.dir = tempfile.mkdtemp(prefix='azol_fuzz_')
        self.ran_x64 = 0
        self.ran_arm = 0

    def build(self, src, out, target):
        try:
            p = subprocess.run([self.azol, 'build', '--target', target, src, '-o', out], capture_output=True, timeout=60)
        except subprocess.TimeoutExpired:
            return 'hang', ''
        text = (p.stdout + p.stderr).decode(errors='replace')
        if p.returncode == 0:
            return 'ok', text
        if p.returncode < 0 or 'panicked' in text:
            return 'crash', text
        return 'error', text

    def check(self, prog):
        """Returns (kind, details); kind is None when everything agrees and
        'skip' when the program is not a fair test."""
        src = render(prog)
        try:
            want = Interp(prog, self.limit).run()
        except Budget:
            return 'skip', {'why': 'too much work'}
        except RecursionError:
            return 'skip', {'why': 'too deep'}
        info = {'source': src, 'oracle': want}
        path = os.path.join(self.dir, 'p.azl')
        with open(path, 'w') as f:
            f.write(src)
        exe = os.path.join(self.dir, 'p.x64')
        state, text = self.build(path, exe, 'x86_64-linux')
        if state != 'ok':
            line = first_error(text)
            if 'slots of stack' in line or 'too large for now' in line:
                return 'skip', {'why': line}
            info['compiler'] = text
            return 'compile-' + state + ': ' + line, info
        self.ran_x64 += 1
        try:
            p = subprocess.run([exe], capture_output=True, timeout=20)
            got = (p.stdout, p.stderr, p.returncode)
        except subprocess.TimeoutExpired:
            info['x64'] = (b'', b'', 'timeout')
            return 'x64-hang', info
        info['x64'] = got
        if got[2] < 0:
            return 'x64-crash (signal %d)' % -got[2], info
        if got != want:
            return 'x64-mismatch', info
        if not self.arm:
            return None, info
        mac = os.path.join(self.dir, 'p.mac')
        state, text = self.build(path, mac, 'aarch64-macos')
        if state != 'ok':
            info['compiler'] = text
            return 'arm64-compile-' + state + ': ' + first_error(text), info
        self.ran_arm += 1
        try:
            out, code, trap, balanced, err, heap = EMU.run(mac, max_insn=60_000_000)
        except Exception as x:
            info['arm64'] = ('', repr(x), 'emulator error')
            return 'arm64-emulator-error', info
        info['arm64'] = (out, err, code, trap, balanced, heap)
        if trap is not None:
            return 'arm64-trap', info
        if (loose(out), loose(err), code) != (loose(want[0]), loose(want[1]), want[2]):
            return 'arm64-mismatch', info
        if heap.get('violations'):
            return 'arm64-heap-violation', info
        if heap.get('leaks'):
            return 'arm64-leak', info
        if not balanced:
            return 'arm64-stack-unbalanced', info
        return None, info


def loose(x):
    """Text for comparing with the emulator, which decodes what a program
    writes one piece at a time: bytes outside ASCII all count the same."""
    if isinstance(x, bytes):
        x = x.decode('latin-1')
    out = []
    for c in x:
        if ord(c) < 128:
            out.append(c)
        elif not out or out[-1] != '\x7f':
            out.append('\x7f')
    return ''.join(out)


def first_error(text):
    for line in text.splitlines():
        if line.startswith('error'):
            return line
    for line in text.splitlines():
        if line.strip():
            return line.strip()
    return 'no message'


# -------------------------------------------------------------- reducing

def all_blocks(prog):
    out = []

    def walk(blk):
        out.append(blk)
        for st in blk:
            k = st[0]
            if k == 'if':
                for _, b in st[1]:
                    walk(b)
                if st[2] is not None:
                    walk(st[2])
            elif k == 'while':
                walk(st[2])
            elif k == 'for':
                walk(st[4])
            elif k == 'match':
                for arm in st[2]:
                    walk(arm[1])
    for f in prog.fns:
        if not f.fixed:
            walk(f.body)
    return out


def all_exprs(prog):
    out = []

    def ex(e):
        out.append(e)
        for x in kids(e):
            ex(x)

    def walk(blk):
        for st in blk:
            k = st[0]
            if k == 'let':
                ex(st[5])
            elif k == 'assign':
                ex(st[2])
            elif k in ('print', 'expr', 'eprint', 'exit'):
                ex(st[1])
            elif k == 'return' and st[1] is not None:
                ex(st[1])
            elif k == 'if':
                for c, b in st[1]:
                    ex(c)
                    walk(b)
                if st[2] is not None:
                    walk(st[2])
            elif k == 'while':
                ex(st[1])
                walk(st[2])
            elif k == 'for':
                ex(st[3])
                walk(st[4])
            elif k == 'match':
                ex(st[1])
                for arm in st[2]:
                    walk(arm[1])
    for f in prog.fns:
        if not f.fixed:
            walk(f.body)
    return out


def size_of(prog):
    return len(render(prog))


def reduce(prog, kind, runner, seconds=90):
    """Shrinks a failing program while the same kind of failure remains."""
    start = time.time()
    tries = [0]

    def fails(p):
        tries[0] += 1
        try:
            k, _ = runner.check(p)
        except OracleError:
            return False
        except Exception:
            return False
        return k == kind

    def late():
        return time.time() - start > seconds

    best = copy.deepcopy(prog)
    progress = True
    while progress and not late():
        progress = False
        # Whole functions that nothing calls any more, and unused declarations.
        text = render(best)
        for f in list(best.fns):
            if f.name != 'main' and len(re.findall(r'\b' + f.name + r'\b', text)) == 1:
                cand = copy.deepcopy(best)
                cand.fns = [g for g in cand.fns if g.name != f.name]
                if fails(cand):
                    best = cand
                    progress = True
        for i in range(len(best.consts) - 1, -1, -1):
            cand = copy.deepcopy(best)
            del cand.consts[i]
            if fails(cand):
                best = cand
                progress = True
        for kind_name in list(reversed(best.decls.order)):
            cand = copy.deepcopy(best)
            cand.decls.order.remove(kind_name)
            if fails(cand):
                best = cand
                progress = True
        # Statements, in runs that halve.
        bi = 0
        while bi < len(all_blocks(best)) and not late():
            n = len(all_blocks(best)[bi])
            chunk = n
            while chunk >= 1 and not late():
                at = 0
                while at < len(all_blocks(best)[bi]):
                    cand = copy.deepcopy(best)
                    blk = all_blocks(cand)[bi]
                    del blk[at:at + chunk]
                    if fails(cand):
                        best = cand
                        progress = True
                    else:
                        at += chunk
                chunk //= 2
            bi += 1
        # A compound statement becomes its body.
        bi = 0
        while bi < len(all_blocks(best)) and not late():
            si = 0
            while si < len(all_blocks(best)[bi]) and not late():
                st = all_blocks(best)[bi][si]
                bodies = []
                if st[0] == 'if':
                    bodies = [b for _, b in st[1]] + ([st[2]] if st[2] is not None else [])
                elif st[0] == 'match':
                    bodies = [arm[1] for arm in st[2] if arm[0][0] in ('_', 'lit')]
                done = False
                for which in range(len(bodies)):
                    cand = copy.deepcopy(best)
                    blk = all_blocks(cand)[bi]
                    st2 = blk[si]
                    if st2[0] == 'if':
                        body = ([b for _, b in st2[1]] + ([st2[2]] if st2[2] is not None else []))[which]
                    else:
                        body = [arm[1] for arm in st2[2] if arm[0][0] in ('_', 'lit')][which]
                    blk[si:si + 1] = body
                    if fails(cand):
                        best = cand
                        progress = True
                        done = True
                        break
                if not done:
                    si += 1
            bi += 1
        # Expressions: a smaller one of the same type, or a plain value.
        ei = 0
        while ei < len(all_exprs(best)) and not late():
            e = all_exprs(best)[ei]
            options = []
            if e.k not in ('int', 'bool', 'str', 'var', 'mut'):
                for j, x in enumerate(kids(e)):
                    if x.t == e.t and x.k != 'mut':
                        options.append(('kid', j))
                if e.t == I64:
                    options.append(('lit', E('int', I64, 0, 'd')))
                    options.append(('lit', E('int', I64, 1, 'd')))
                elif e.t == BOOL:
                    options.append(('lit', E('bool', BOOL, True)))
                    options.append(('lit', E('bool', BOOL, False)))
                elif e.t == STR:
                    options.append(('lit', E('str', STR, b'')))
                    options.append(('lit', E('str', STR, b'a')))
                elif e.t is not None and e.t[0] == 'list' and e.k != 'llit':
                    options.append(('lit', E('llit', e.t, [])))
            elif e.k == 'var' and e.t in (I64, BOOL, STR):
                options.append(('lit', {I64: E('int', I64, 0, 'd'), BOOL: E('bool', BOOL, False), STR: E('str', STR, b'a')}[e.t]))
            elif e.k == 'int' and e.a[0] not in (0, 1):
                options.append(('lit', E('int', I64, 0, 'd')))
                options.append(('lit', E('int', I64, 1, 'd')))
            elif e.k == 'str' and len(e.a[0]) > 1:
                options.append(('lit', E('str', STR, b'a')))
            changed = False
            for what, arg in options:
                cand = copy.deepcopy(best)
                node = all_exprs(cand)[ei]
                new = kids(node)[arg] if what == 'kid' else arg
                node.k, node.t, node.a = new.k, new.t, new.a
                if fails(cand):
                    best = cand
                    progress = True
                    changed = True
                    break
            if not changed:
                ei += 1
    return best, tries[0]


# ------------------------------------------------------------------ main

def show_result(r):
    if r is None:
        return '(not run)\n'
    out = []
    names = ['stdout', 'stderr', 'exit code', 'trap', 'stack balanced', 'heap']
    for name, v in zip(names, r):
        if isinstance(v, bytes):
            v = v.decode('latin-1')
        out.append('--- ' + name + '\n' + str(v) + ('\n' if not str(v).endswith('\n') else ''))
    return ''.join(out)


def save_finding(folder, seed, kind, prog, info, reduced, rinfo, tries):
    name = ''.join(c if c.isalnum() else '_' for c in kind.split(':')[0])[:40] + '_' + str(seed)
    where = os.path.join(folder, name)
    os.makedirs(where, exist_ok=True)
    with open(os.path.join(where, 'original.azl'), 'w') as f:
        f.write(info['source'])
    with open(os.path.join(where, 'reduced.azl'), 'w') as f:
        f.write(rinfo['source'])
    for label, i in (('original', info), ('reduced', rinfo)):
        with open(os.path.join(where, label + '.results.txt'), 'w') as f:
            f.write('seed: %d\nfinding: %s\n\n' % (seed, kind))
            f.write('=== oracle\n' + show_result(i.get('oracle')))
            f.write('\n=== x86_64-linux, run natively\n' + show_result(i.get('x64')))
            f.write('\n=== aarch64-macos, run under the checking emulator\n' + show_result(i.get('arm64')))
            if 'compiler' in i:
                f.write('\n=== compiler\n' + i['compiler'])
    with open(os.path.join(where, 'info.txt'), 'w') as f:
        f.write('seed: %d\nfinding: %s\nreduced from %d to %d bytes in %d runs\nreproduce: python3 tests/fuzz/fuzz.py --only %d\n'
                % (seed, kind, len(info['source']), len(rinfo['source']), tries, seed))
    return where


def main():
    ap = argparse.ArgumentParser(description='A differential fuzzer for the azoL compiler.')
    ap.add_argument('--count', type=int, default=200, help='how many programs to run')
    ap.add_argument('--seed', type=int, default=1, help='the first seed; program i uses seed + i')
    ap.add_argument('--minutes', type=float, default=25, help='stop fuzzing after this long')
    ap.add_argument('--azol', default=os.path.join(REPO, 'target', 'release', 'azol'), help='the compiler to test')
    ap.add_argument('--no-arm', action='store_true', help='skip the arm64 build and the emulator')
    ap.add_argument('--no-reduce', action='store_true', help='save findings without shrinking them')
    ap.add_argument('--findings', default=os.path.join(HERE, 'findings'), help='where findings are saved')
    ap.add_argument('--max-findings', type=int, default=40, help='stop after this many findings')
    ap.add_argument('--show', type=int, help='print the program of one seed and stop')
    ap.add_argument('--only', type=int, help='run the program of one seed and print what happened')
    ap.add_argument('--quiet', action='store_true')
    a = ap.parse_args()

    if a.show is not None:
        print(render(generate(a.show)[0]))
        return 0
    if not os.path.exists(a.azol):
        print('no compiler at ' + a.azol + '; run `cargo build --release` first')
        return 2
    have_emu = (not a.no_arm) and load_emulator()
    runner = Runner(a.azol, arm=have_emu)
    if a.only is not None:
        try:
            prog, _ = generate(a.only)
        except (RecursionError, Retry):
            print('result: skip (the generator gave up on this seed)')
            return 0
        kind, info = runner.check(prog)
        print('result:', kind or 'all agree', info.get('why', ''))
        if kind and kind != 'skip':
            print('=== oracle\n' + show_result(info.get('oracle')))
            print('=== x86_64-linux\n' + show_result(info.get('x64')))
            print('=== aarch64-macos\n' + show_result(info.get('arm64')))
            print(info.get('compiler', ''))
        return 0 if kind in (None, 'skip') else 1

    start = time.time()
    stats = Counter()
    kinds = Counter()
    ran = skipped = broken = 0
    stops = 0
    found = []
    seed = a.seed
    while ran < a.count and time.time() - start < a.minutes * 60 and len(found) < a.max_findings:
        s = seed
        seed += 1
        try:
            prog, st = generate(s)
        except (RecursionError, Retry):
            skipped += 1
            continue
        try:
            kind, info = runner.check(prog)
        except OracleError as x:
            print('seed %d: the generator wrote a program its own oracle rejects: %s' % (s, x), flush=True)
            broken += 1
            continue
        if kind == 'skip':
            skipped += 1
            continue
        ran += 1
        stats.update(st)
        if info['oracle'][1]:
            stops += 1
        if kind is not None:
            kinds[kind.split(':')[0]] += 1
            print('seed %d: %s' % (s, kind), flush=True)
            if a.no_reduce:
                reduced, rinfo, tries = prog, info, 0
            else:
                reduced, tries = reduce(prog, kind, runner)
                _, rinfo = runner.check(reduced)
            where = save_finding(a.findings, s, kind, prog, info, reduced, rinfo, tries)
            found.append((s, kind, where))
            print('   saved in ' + os.path.relpath(where), flush=True)
        if not a.quiet and ran % 100 == 0:
            print('%d programs, %d findings, %d skipped, %.0f s' % (ran, len(found), skipped, time.time() - start), flush=True)
    took = time.time() - start
    print()
    print('programs run: %d (x86_64-linux: %d, aarch64-macos under the emulator: %d), skipped: %d, seeds %d..%d, %.0f s'
          % (ran, runner.ran_x64, runner.ran_arm, skipped, a.seed, seed - 1, took))
    print('programs that ended in a predicted run-time stop or wrote to stderr: %d' % stops)
    if broken:
        print('programs the oracle rejected (generator bugs): %d' % broken)
    print('findings: %d' % len(found))
    for s, kind, where in found:
        print('   seed %d: %s -> %s' % (s, kind, os.path.relpath(where)))
    if not a.quiet:
        print()
        print('what the programs contained (occurrences):')
        for name, n in sorted(stats.items()):
            print('   %-36s %d' % (name, n))
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main())
