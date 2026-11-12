# Editor support for azoL

## VS Code

`vscode/` is a small extension that highlights `.azl` files: keywords,
types, constants, numbers (decimal, `0x`, `0b`), strings and their escapes,
character literals, comments, and function and type names. It also sets up
`//` comments, bracket matching and auto-closing pairs. It has no code and
no dependencies.

Install it for your user, from the root of the repository:

    mkdir -p ~/.vscode/extensions
    cp -r editors/vscode ~/.vscode/extensions/durasi.azol-0.1.0

Then restart VS Code and open a `.azl` file. To remove it, delete that
folder.

The same grammar file, `vscode/syntaxes/azol.tmLanguage.json`, works in
other editors that read TextMate grammars.

### Checking the extension

    python3 editors/vscode/test_grammar.py

The script needs only Python 3. It checks that the JSON files parse and
have the fields VS Code needs, that every regular expression compiles, and
that sample lines of azoL get the expected scopes.
