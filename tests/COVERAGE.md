# What replaces which test of the first compiler

The first azoL compiler was written in Rust, and its tests ran with `cargo test`: 143 test functions
inside `src/*.rs` (141 run on one machine; two are for the other processor) and 14 tests in `tests/build_run.rs` and `tests/format.rs`. This table says, for
each of them, what in the suite that needs no Rust (`sh test.sh`, `tests/run.py`) requires the same
thing, or why nothing does. "Not carried over" is written in bold wherever a part was dropped.

How the cases were made: the unit tests were run once with the lexer, the parser and the checker
recording every source text given to them and the result. Each text became a case; the result the
test saw became its `asserted` line (message, line and column), which `--update-golden` never
rewrites. The full reports (`stdout`, `stderr`) are what the Rust-built compiler printed for the
case. The tests of `src/main.rs` and `src/diag.rs` were written out by hand from their source.

The unit tests of the checker ran without the standard library; the command line always has it.
Two cases are right programs because of that and say so in a `note` (`Map` as a type name, the
method `trim`).

## src/lexer.rs (14 tests)

| Test | Replaced by |
|---|---|
| `empty_source` | `tests/golden/lex/empty_source.cases` (1 case) |
| `function_header` | `tests/golden/lex/function_header.cases` (1 case) |
| `keywords_and_identifiers` | `tests/golden/lex/keywords_and_identifiers.cases` (1 case) |
| `numbers` | `tests/golden/lex/numbers.cases` (1 case) |
| `operators` | `tests/golden/lex/operators.cases` (1 case) |
| `strings_and_escapes` | `tests/golden/lex/strings_and_escapes.cases` (1 case) |
| `comments_and_newlines` | `tests/golden/lex/comments_and_newlines.cases` (1 case) |
| `non_ascii_identifier` | `tests/golden/lex/non_ascii_identifier.cases` (1 case) |
| `unterminated_string` | `tests/errors/lexer__unterminated_string.cases` (1 case) |
| `unknown_escape` | `tests/errors/lexer__unknown_escape.cases` (1 case) |
| `unexpected_character` | `tests/errors/lexer__unexpected_character.cases` (1 case) |
| `bit_operators` | `tests/golden/lex/bit_operators.cases` (2 cases) |
| `hexadecimal_and_binary_numbers` | `tests/errors/lexer__hexadecimal_and_binary_numbers.cases` (10 cases); `tests/golden/lex/hexadecimal_and_binary_numbers.cases` (1 case) |
| `character_literals` | `tests/errors/lexer__character_literals.cases` (9 cases); `tests/golden/lex/character_literals.cases` (1 case) |

## src/parser.rs (26 tests)

| Test | Replaced by |
|---|---|
| `multiplication_binds_tighter_than_addition` | `tests/golden/parse/multiplication_binds_tighter_than_addition.cases` (2 cases) |
| `binary_operators_are_left_associative` | `tests/golden/parse/binary_operators_are_left_associative.cases` (2 cases) |
| `logic_and_comparison_precedence` | `tests/golden/parse/logic_and_comparison_precedence.cases` (2 cases) |
| `bit_operator_precedence` | `tests/golden/parse/bit_operator_precedence.cases` (8 cases) |
| `number_and_character_literals_in_patterns` | `tests/golden/parse/number_and_character_literals_in_patterns.cases` (1 case) |
| `unary_binds_tighter_than_binary` | `tests/golden/parse/unary_binds_tighter_than_binary.cases` (2 cases) |
| `postfix_chains` | `tests/golden/parse/postfix_chains.cases` (4 cases) |
| `let_and_var_with_types` | `tests/golden/parse/let_and_var_with_types.cases` (1 case) |
| `struct_literal_is_not_parsed_in_conditions` | `tests/golden/parse/struct_literal_is_not_parsed_in_conditions.cases` (3 cases) |
| `struct_literal_in_a_condition_says_what_to_do` | `tests/errors/parser__struct_literal_in_a_condition_says_what_to_do.cases` (4 cases); `tests/golden/parse/struct_literal_in_a_condition_says_what_to_do.cases` (2 cases) |
| `else_if_chain_and_else_on_next_line` | `tests/golden/parse/else_if_chain_and_else_on_next_line.cases` (1 case) |
| `for_loops` | `tests/golden/parse/for_loops.cases` (2 cases) |
| `match_arms` | `tests/golden/parse/match_arms.cases` (1 case) |
| `multi_line_expressions` | `tests/golden/parse/multi_line_expressions.cases` (4 cases) |
| `a_new_line_does_not_continue_a_call_or_index` | `tests/golden/parse/a_new_line_does_not_continue_a_call_or_index.cases` (1 case) |
| `items` | `tests/golden/parse/items.cases` (1 case) |
| `constants` | `tests/errors/parser__constants.cases` (4 cases); `tests/golden/parse/constants.cases` (1 case) |
| `empty_file_and_comments_only` | `tests/golden/parse/empty_file_and_comments_only.cases` (2 cases) |
| `error_missing_closing_paren` | `tests/errors/parser__error_missing_closing_paren.cases` (1 case) |
| `error_two_statements_on_one_line` | `tests/errors/parser__error_two_statements_on_one_line.cases` (1 case) |
| `error_invalid_assignment_target` | `tests/errors/parser__error_invalid_assignment_target.cases` (1 case) |
| `error_statement_outside_function` | `tests/errors/parser__error_statement_outside_function.cases` (1 case) |
| `error_unclosed_block` | `tests/errors/parser__error_unclosed_block.cases` (1 case) |
| `error_missing_expression` | `tests/errors/parser__error_missing_expression.cases` (1 case) |
| `deep_nesting_is_an_error_not_a_crash` | `tests/run.py`, section `errors`: 5000 nested parentheses and 5000 minus signs, through `parse` and `check`; each must report `error[E0802]: code is nested too deeply`. |
| `damaged_input_never_panics` | `tests/run.py`, section `errors`: every beginning and the same 4000 corruptions of `examples/point.azl`, read by `azol fmt --check` (lexer and parser); every 40th also by `azol check`. None may end the compiler by a signal. |

## src/sema.rs (44 tests)

| Test | Replaced by |
|---|---|
| `accepts_a_valid_program_and_counts_slots` | `tests/golden/check/sema__accepts_a_valid_program_and_counts_slots.cases` (1 case). The test also looked at tables inside the checker (slots, offsets, moved variables, decoded text). They cannot be seen from outside; a change in them changes the executables that `golden/check` pins for all three targets. |
| `string_literals_are_decoded` | `tests/golden/check/sema__string_literals_are_decoded.cases` (1 case). The test also looked at tables inside the checker (slots, offsets, moved variables, decoded text). They cannot be seen from outside; a change in them changes the executables that `golden/check` pins for all three targets. |
| `struct_layout` | `tests/golden/check/sema__struct_layout.cases` (1 case). The test also looked at tables inside the checker (slots, offsets, moved variables, decoded text). They cannot be seen from outside; a change in them changes the executables that `golden/check` pins for all three targets. |
| `reports_position_of_the_problem` | `tests/errors/sema__reports_position_of_the_problem.cases` (1 case) |
| `type_errors` | `tests/errors/sema__type_errors.cases` (21 cases) |
| `variable_errors` | `tests/errors/sema__variable_errors.cases` (6 cases) |
| `function_errors` | `tests/errors/sema__function_errors.cases` (7 cases) |
| `return_errors` | `tests/errors/sema__return_errors.cases` (5 cases); `tests/golden/check/sema__return_errors.cases` (1 case) |
| `loop_and_literal_errors` | `tests/errors/sema__loop_and_literal_errors.cases` (2 cases); `tests/golden/check/sema__loop_and_literal_errors.cases` (1 case) |
| `struct_declaration_errors` | `tests/errors/sema__struct_declaration_errors.cases` (5 cases) |
| `struct_value_errors` | `tests/errors/sema__struct_value_errors.cases` (13 cases); `tests/golden/check/sema__struct_value_errors.cases` (1 case) |
| `parameters_are_read_only_unless_mut` | `tests/errors/sema__parameters_are_read_only_unless_mut.cases` (6 cases); `tests/golden/check/sema__parameters_are_read_only_unless_mut.cases` (2 cases) |
| `one_call_cannot_get_overlapping_places_when_one_is_mut` | `tests/errors/sema__one_call_cannot_get_overlapping_places_when_one_is_mut.cases` (7 cases); `tests/golden/check/sema__one_call_cannot_get_overlapping_places_when_one_is_mut.cases` (7 cases) |
| `list_programs_that_are_accepted` | `tests/golden/check/sema__list_programs_that_are_accepted.cases` (6 cases) |
| `list_type_errors` | `tests/errors/sema__list_type_errors.cases` (16 cases) |
| `lists_can_only_be_changed_through_changeable_places` | `tests/errors/sema__lists_can_only_be_changed_through_changeable_places.cases` (7 cases) |
| `a_list_cannot_change_while_something_refers_to_it` | `tests/errors/sema__a_list_cannot_change_while_something_refers_to_it.cases` (9 cases); `tests/golden/check/sema__a_list_cannot_change_while_something_refers_to_it.cases` (2 cases) |
| `ownership_decisions` | `tests/golden/check/sema__ownership_decisions.cases` (1 case). The test also looked at tables inside the checker (slots, offsets, moved variables, decoded text). They cannot be seen from outside; a change in them changes the executables that `golden/check` pins for all three targets. |
| `strings_and_nested_lists_are_accepted` | `tests/golden/check/sema__strings_and_nested_lists_are_accepted.cases` (4 cases) |
| `string_errors` | `tests/errors/sema__string_errors.cases` (5 cases) |
| `a_string_cannot_change_while_an_operator_still_reads_it` | `tests/errors/sema__a_string_cannot_change_while_an_operator_still_reads_it.cases` (4 cases); `tests/golden/check/sema__a_string_cannot_change_while_an_operator_still_reads_it.cases` (2 cases) |
| `a_loop_variable_is_a_view_and_is_never_moved_out` | `tests/golden/check/sema__a_loop_variable_is_a_view_and_is_never_moved_out.cases` (1 case). The test also looked at tables inside the checker (slots, offsets, moved variables, decoded text). They cannot be seen from outside; a change in them changes the executables that `golden/check` pins for all three targets. |
| `enum_layout_and_ownership` | `tests/golden/check/sema__enum_layout_and_ownership.cases` (1 case). The test also looked at tables inside the checker (slots, offsets, moved variables, decoded text). They cannot be seen from outside; a change in them changes the executables that `golden/check` pins for all three targets. |
| `enum_declaration_errors` | `tests/errors/sema__enum_declaration_errors.cases` (23 cases); `tests/golden/check/sema__enum_declaration_errors.cases` (1 case) |
| `enum_value_errors` | `tests/errors/sema__enum_value_errors.cases` (17 cases); `tests/golden/check/sema__enum_value_errors.cases` (2 cases) |
| `match_errors` | `tests/errors/sema__match_errors.cases` (38 cases); `tests/golden/check/sema__match_errors.cases` (4 cases) |
| `system_functions` | `tests/errors/sema__system_functions.cases` (32 cases); `tests/golden/check/sema__system_functions.cases` (5 cases) |
| `embedded_files` | `tests/errors/sema__embedded_files.cases` (9 cases); `tests/golden/check/sema__embedded_files.cases` (2 cases). The test used a file with an absolute path; the cases use paths next to the source and in another folder, which is what the absolute path stood for. |
| `is_utf8_is_a_method_of_strings` | `tests/errors/sema__is_utf8_is_a_method_of_strings.cases` (2 cases); `tests/golden/check/sema__is_utf8_is_a_method_of_strings.cases` (1 case) |
| `string_bytes_and_slices` | `tests/errors/sema__string_bytes_and_slices.cases` (9 cases); `tests/golden/check/sema__string_bytes_and_slices.cases` (1 case) |
| `library_methods` | `tests/errors/sema__library_methods.cases` (12 cases); `tests/golden/check/sema__library_methods.cases` (3 cases) |
| `library_methods_that_change_their_receiver` | `tests/errors/sema__library_methods_that_change_their_receiver.cases` (9 cases); `tests/golden/check/sema__library_methods_that_change_their_receiver.cases` (4 cases) |
| `how_numbers_are_written` | `tests/errors/sema__how_numbers_are_written.cases` (12 cases); `tests/golden/check/sema__how_numbers_are_written.cases` (1 case) |
| `the_smallest_number_can_be_written` | `tests/errors/sema__the_smallest_number_can_be_written.cases` (3 cases); `tests/golden/check/sema__the_smallest_number_can_be_written.cases` (2 cases) |
| `the_try_operator` | `tests/errors/sema__the_try_operator.cases` (10 cases); `tests/golden/check/sema__the_try_operator.cases` (8 cases) |
| `library_names_and_the_programs_own` | `tests/errors/sema__library_names_and_the_programs_own.cases` (4 cases); `tests/golden/check/sema__library_names_and_the_programs_own.cases` (2 cases) |
| `generic_functions` | `tests/errors/sema__generic_functions.cases` (22 cases); `tests/golden/check/sema__generic_functions.cases` (6 cases) |
| `generic_structs` | `tests/errors/sema__generic_structs.cases` (14 cases); `tests/golden/check/sema__generic_structs.cases` (2 cases) |
| `last_use_of_a_variable_moves` | `tests/golden/check/sema__last_use_of_a_variable_moves.cases` (28 cases). The test also looked at tables inside the checker (slots, offsets, moved variables, decoded text). They cannot be seen from outside; a change in them changes the executables that `golden/check` pins for all three targets. |
| `appending_to_a_string_in_place` | `tests/golden/check/sema__appending_to_a_string_in_place.cases` (6 cases). The test also looked at tables inside the checker (slots, offsets, moved variables, decoded text). They cannot be seen from outside; a change in them changes the executables that `golden/check` pins for all three targets. |
| `bit_operators_and_number_forms` | `tests/errors/sema__bit_operators_and_number_forms.cases` (16 cases); `tests/golden/check/sema__bit_operators_and_number_forms.cases` (8 cases) |
| `constants` | `tests/errors/sema__constants.cases` (48 cases); `tests/golden/check/sema__constants.cases` (6 cases) |
| `equality_of_enums_without_values` | `tests/errors/sema__equality_of_enums_without_values.cases` (7 cases); `tests/golden/check/sema__equality_of_enums_without_values.cases` (1 case) |
| `features_not_built_yet_are_reported_clearly` | `tests/errors/sema__features_not_built_yet_are_reported_clearly.cases` (2 cases) |

## src/diag.rs (9 tests)

| Test | Replaced by |
|---|---|
| `positions_are_found_in_the_right_file` | `tests/errors/main__every_file_reports_its_first_syntax_error.cases`, `main__errors_are_sorted_by_file_and_position.cases`, `main__a_fix_in_a_later_file_is_placed_inside_that_file.cases` and the multi-file checks of section `programs`: errors in the first, a middle and the last file must name that file and the line and column inside it. The function itself (`locate`) is not reachable from outside; its arithmetic on bare offsets is **not carried over** as such. |
| `patterns_fit_their_messages_only` | `tests/errors/diag__messages_get_the_code_of_their_kind.cases` (34 cases): the pattern matcher is internal; what it decides is the code of a message, and those are required there message by message, among them the four families this test used (`unknown variable`, `needs ... but this is`, `takes N argument(s)`, `has no fields`). |
| `messages_get_the_code_of_their_kind` | `tests/errors/diag__messages_get_the_code_of_their_kind.cases`: 34 of the 35 message-code pairs, each as a program that gets the message, required with a `contains` line that `--update-golden` cannot rewrite. **Not carried over:** the pair for a text that is no message of the compiler (`E0000`), which cannot be produced through the command line; instead every JSON report of every case is checked not to carry `E0000`. |
| `every_message_of_the_compiler_has_a_code` | **Partly carried over.** The test scanned the Rust sources for message texts. Without those sources the suite checks the result instead: no report of any case may carry `E0000`, and the suite reaches 73 of the 74 codes (all but E0801, a source file of 4 GiB). A new message without a code is caught as soon as a case reaches it, not before. |
| `codes_are_unique_and_well_formed` | `tests/run.py`, section `errors`, "The error codes": `azol explain` must list every code once, in order, as `E0` and three digits, each with a rule that ends in a full stop. That every pattern of the table maps back to its own code is internal and **not carried over**; `diag__messages_get_the_code_of_their_kind.cases` covers the 34 messages named by the tests. |
| `explanations_exist_for_every_code` | `tests/run.py`, section `errors`: `azol explain <code>` for every listed code starts with the code and has "Messages with this code:"; the 17 codes of the ownership and `match` rules have "Wrong" and "Right"; `explain E9999` fails; the list is sorted. |
| `the_error_reference_is_up_to_date` | `tests/run.py`, section `errors`: `docs/errors.md` must equal the output of `azol explain --markdown`. |
| `json_strings_are_escaped` | `tests/errors/diag__json_strings_are_escaped.cases` and `main__json_output_has_every_field.cases`: quotes, backslashes, a tab, a carriage return, the control characters 1 and 31 and non-ASCII text in a report. In addition every line of every `--json` run of the suite is read with Python's JSON parser and its `source` field compared with the line of the file. |
| `edits_are_applied_by_line_and_column` | `tests/run.py` applies fixes itself (`apply_edit`, lines and columns in characters) in `main__fixes_resolve_their_errors.cases` (13 cases, one with `çay` before the place and one indented with tabs) and `main__a_fix_in_a_later_file_is_placed_inside_that_file.cases`; the fixed file must hold the required text and pass `check`. The Rust function `apply` goes away with the Rust compiler; an edit that spans two lines is **not carried over** (no fix of the compiler spans lines). |

## src/main.rs (12 tests)

| Test | Replaced by |
|---|---|
| `every_function_reports_its_first_error` | `tests/errors/main__every_function_reports_its_first_error.cases` (2 cases) |
| `a_missing_main_is_reported_with_the_other_errors` | `tests/errors/main__a_missing_main_is_reported_with_the_other_errors.cases` (1 case) |
| `an_error_in_a_declaration_is_reported_alone` | `tests/errors/main__an_error_in_a_declaration_is_reported_alone.cases` (1 case) |
| `a_generic_function_reports_a_mistake_once` | `tests/errors/main__a_generic_function_reports_a_mistake_once.cases` (1 case) |
| `every_file_reports_its_first_syntax_error` | `tests/errors/main__every_file_reports_its_first_syntax_error.cases` (2 cases) |
| `errors_are_sorted_by_file_and_position` | `tests/errors/main__errors_are_sorted_by_file_and_position.cases` (1 case) |
| `a_report_covers_the_whole_expression_or_name` | `tests/errors/main__a_report_covers_the_whole_expression_or_name.cases` (7 cases) |
| `fixes_resolve_their_errors` | `tests/errors/main__fixes_resolve_their_errors.cases` (13 cases) |
| `no_fix_is_offered_when_it_would_not_be_the_whole_repair` | `tests/errors/main__no_fix_is_offered_when_it_would_not_be_the_whole_repair.cases` (5 cases) |
| `json_output_has_every_field` | `tests/errors/main__json_output_has_every_field.cases` (2 cases) |
| `a_fix_in_a_later_file_is_placed_inside_that_file` | `tests/errors/main__a_fix_in_a_later_file_is_placed_inside_that_file.cases` (1 case) |
| `build_arguments_accept_several_files_in_any_order` | `tests/errors/command_line.cases` (22 cases): each accepted and each refused argument list of the test as a real command line (exit code 0 and the files written, or exit code 2 and the usage text), plus other command lines. |

## src/format.rs (8 tests)

| Test | Replaced by |
|---|---|
| `repository_files_keep_their_tokens_comments_and_tree` | `tests/run.py`, section `format`, `guarantees`: for every `.azl` file of the repository the tokens (`azol lex`, without positions and line ends), the comments and the tree (`azol parse`) are the same before and after `azol fmt -`. |
| `formatting_is_idempotent` | `tests/run.py`, section `format`, `guarantees`: formatting the formatted text changes nothing, for every `.azl` file of the repository. |
| `output_has_no_stray_whitespace` | `tests/run.py`, section `format`, `guarantees`: no space at the end of a line, no tab in an indentation, no blank line at the start or the end, never two blank lines, one line break at the end. |
| `a_file_that_does_not_parse_is_refused_with_the_parser_error` | `tests/golden/format/a_file_that_does_not_parse_is_refused_with_the_parser_error.cases` (1 case) |
| `a_file_that_does_not_lex_is_refused_with_the_lexer_error` | `tests/golden/format/a_file_that_does_not_lex_is_refused_with_the_lexer_error.cases` (1 case) |
| `empty_and_comment_only_files` | `tests/golden/format/empty_and_comment_only_files.cases` (4 cases) |
| `minus_is_unary_or_binary_by_what_stands_before_it` | `tests/golden/format/minus_is_unary_or_binary_by_what_stands_before_it.cases` (10 cases) |
| `braces_of_a_condition_open_a_block_not_a_struct_literal` | `tests/golden/format/braces_of_a_condition_open_a_block_not_a_struct_literal.cases` (6 cases) |

## src/macho.rs (5 tests)

| Test | Replaced by |
|---|---|
| `a_program_starts_in_the_runtime_which_calls_main` | `tests/run.py`, `macho_problem`, on every Mach-O file the suite builds (section `cross` and every `build-all` case): header, load commands and segments inside the file, the entry point inside `__TEXT`, and the signature: SHA-256, ad hoc, the right number of page hashes, the name it is made for, and **every page hash recomputed with Python's hashlib**. The exact bytes are pinned by `tests/golden/executables.txt`. |
| `layout_is_consistent` | `tests/run.py`, `macho_problem`, on every Mach-O file the suite builds (section `cross` and every `build-all` case): header, load commands and segments inside the file, the entry point inside `__TEXT`, and the signature: SHA-256, ad hoc, the right number of page hashes, the name it is made for, and **every page hash recomputed with Python's hashlib**. The exact bytes are pinned by `tests/golden/executables.txt`. |
| `adrp_reaches_other_pages_in_both_directions` | `tests/run.py`, `macho_problem`, on every Mach-O file the suite builds (section `cross` and every `build-all` case): header, load commands and segments inside the file, the entry point inside `__TEXT`, and the signature: SHA-256, ad hoc, the right number of page hashes, the name it is made for, and **every page hash recomputed with Python's hashlib**. The exact bytes are pinned by `tests/golden/executables.txt`. |
| `every_page_gets_its_own_hash` | `tests/run.py`, `macho_problem`, on every Mach-O file the suite builds (section `cross` and every `build-all` case): header, load commands and segments inside the file, the entry point inside `__TEXT`, and the signature: SHA-256, ad hoc, the right number of page hashes, the name it is made for, and **every page hash recomputed with Python's hashlib**. The exact bytes are pinned by `tests/golden/executables.txt`. |
| `signature_hashes_every_page` | `tests/run.py`, `macho_problem`, on every Mach-O file the suite builds (section `cross` and every `build-all` case): header, load commands and segments inside the file, the entry point inside `__TEXT`, and the signature: SHA-256, ad hoc, the right number of page hashes, the name it is made for, and **every page hash recomputed with Python's hashlib**. The exact bytes are pinned by `tests/golden/executables.txt`. |

## src/elf.rs (5 tests)

| Test | Replaced by |
|---|---|
| `the_runtime_matches_the_layout` | `tests/run.py`, `elf_problem`, on every x86-64 ELF file the suite builds: 64-bit little-endian static executable for the right machine, no interpreter, segments inside the file, on their alignment and not overlapping, none both writable and executable, the entry point inside an executable segment. The exact bytes are pinned by `tests/golden/executables.txt`; on x86-64 Linux every test program is also run. |
| `layout_places_the_parts_in_order` | `tests/run.py`, `elf_problem`, on every x86-64 ELF file the suite builds: 64-bit little-endian static executable for the right machine, no interpreter, segments inside the file, on their alignment and not overlapping, none both writable and executable, the entry point inside an executable segment. The exact bytes are pinned by `tests/golden/executables.txt`; on x86-64 Linux every test program is also run. |
| `header_describes_a_static_x86_64_executable` | `tests/run.py`, `elf_problem`, on every x86-64 ELF file the suite builds: 64-bit little-endian static executable for the right machine, no interpreter, segments inside the file, on their alignment and not overlapping, none both writable and executable, the entry point inside an executable segment. The exact bytes are pinned by `tests/golden/executables.txt`; on x86-64 Linux every test program is also run. |
| `contents_and_references_are_resolved` | `tests/run.py`, `elf_problem`, on every x86-64 ELF file the suite builds: 64-bit little-endian static executable for the right machine, no interpreter, segments inside the file, on their alignment and not overlapping, none both writable and executable, the entry point inside an executable segment. The exact bytes are pinned by `tests/golden/executables.txt`; on x86-64 Linux every test program is also run. |
| `relative_distances` | `tests/run.py`, `elf_problem`, on every x86-64 ELF file the suite builds: 64-bit little-endian static executable for the right machine, no interpreter, segments inside the file, on their alignment and not overlapping, none both writable and executable, the entry point inside an executable segment. The exact bytes are pinned by `tests/golden/executables.txt`; on x86-64 Linux every test program is also run. |

## src/elf_arm64.rs (7 tests)

| Test | Replaced by |
|---|---|
| `the_library_matches_the_layout_and_the_imports` | As for `elf`, with the machine ARM64. The exact bytes are pinned by `tests/golden/executables.txt`. Nothing in this suite runs these files unless the test machine is ARM64 Linux. |
| `layout_places_the_parts_in_order` | As for `elf`, with the machine ARM64. The exact bytes are pinned by `tests/golden/executables.txt`. Nothing in this suite runs these files unless the test machine is ARM64 Linux. |
| `header_describes_a_static_arm64_executable` | As for `elf`, with the machine ARM64. The exact bytes are pinned by `tests/golden/executables.txt`. Nothing in this suite runs these files unless the test machine is ARM64 Linux. |
| `contents_and_references_are_resolved` | As for `elf`, with the machine ARM64. The exact bytes are pinned by `tests/golden/executables.txt`. Nothing in this suite runs these files unless the test machine is ARM64 Linux. |
| `every_import_gets_a_direct_call` | As for `elf`, with the machine ARM64. The exact bytes are pinned by `tests/golden/executables.txt`. Nothing in this suite runs these files unless the test machine is ARM64 Linux. |
| `branch_distances` | As for `elf`, with the machine ARM64. The exact bytes are pinned by `tests/golden/executables.txt`. Nothing in this suite runs these files unless the test machine is ARM64 Linux. |
| `adrp_reaches_other_pages_in_both_directions` | As for `elf`, with the machine ARM64. The exact bytes are pinned by `tests/golden/executables.txt`. Nothing in this suite runs these files unless the test machine is ARM64 Linux. |

## src/sha256.rs (7 tests)

| Test | Replaced by |
|---|---|
| `known_vectors` | `compiler/sha256_test.azl` is built and run in section `programs` (seven known hashes, lengths around the block boundary). And every page hash of every Mach-O signature is recomputed with Python's hashlib. |
| `portable_version_matches_known_vectors` | **Not carried over.** It compares the two SHA-256 implementations of the Rust compiler (CPU instructions and portable code) with each other or checks the choice between them. Both go away with it. The implementation in azoL has one path; see `known_vectors`. |
| `hardware_version_matches_known_vectors` | **Not carried over.** It compares the two SHA-256 implementations of the Rust compiler (CPU instructions and portable code) with each other or checks the choice between them. Both go away with it. The implementation in azoL has one path; see `known_vectors`. |
| `hardware_version_matches_portable_version_at_every_length` | **Not carried over.** It compares the two SHA-256 implementations of the Rust compiler (CPU instructions and portable code) with each other or checks the choice between them. Both go away with it. The implementation in azoL has one path; see `known_vectors`. |
| `hardware_version_matches_portable_version_block_by_block` | **Not carried over.** It compares the two SHA-256 implementations of the Rust compiler (CPU instructions and portable code) with each other or checks the choice between them. Both go away with it. The implementation in azoL has one path; see `known_vectors`. |
| `self_check_accepts_the_hardware_version` | **Not carried over.** It compares the two SHA-256 implementations of the Rust compiler (CPU instructions and portable code) with each other or checks the choice between them. Both go away with it. The implementation in azoL has one path; see `known_vectors`. |
| `apple_silicon_uses_the_hardware_version` | **Not carried over.** It compares the two SHA-256 implementations of the Rust compiler (CPU instructions and portable code) with each other or checks the choice between them. Both go away with it. The implementation in azoL has one path; see `known_vectors`. |

## src/x64.rs (5 tests)

| Test | Replaced by |
|---|---|
| `dump_for_harness` | **Not carried over as a table of encodings.** The test compared instruction bytes with a reference assembler. Now every instruction the generator emits for the 105 programs of section `cross` and the 120 of `golden/check` is pinned by hash for all three targets, so no encoding can change unnoticed, and on the machine of the test run the programs are executed, which shows a wrong encoding of that machine. A wrong encoding in new code for another machine is not caught here; the fuzzer (`tests/fuzz`) with an emulator is the tool for that. |
| `leaner_forms_match_the_reference_assembler` | **Not carried over as a table of encodings.** The test compared instruction bytes with a reference assembler. Now every instruction the generator emits for the 105 programs of section `cross` and the 120 of `golden/check` is pinned by hash for all three targets, so no encoding can change unnoticed, and on the machine of the test run the programs are executed, which shows a wrong encoding of that machine. A wrong encoding in new code for another machine is not caught here; the fuzzer (`tests/fuzz`) with an emulator is the tool for that. |
| `encodings_match_the_reference_assembler` | **Not carried over as a table of encodings.** The test compared instruction bytes with a reference assembler. Now every instruction the generator emits for the 105 programs of section `cross` and the 120 of `golden/check` is pinned by hash for all three targets, so no encoding can change unnoticed, and on the machine of the test run the programs are executed, which shows a wrong encoding of that machine. A wrong encoding in new code for another machine is not caught here; the fuzzer (`tests/fuzz`) with an emulator is the tool for that. |
| `division_by_a_constant_matches_real_division` | `tests/run.py`, section `programs`, `division_program`: a generated program divides 377 numbers (the test's edge values and generator, and the values around multiples) by each of the test's 300 divisors as a constant and as a variable, and prints a checksum that Python works out independently. It runs on the machine of the test run, so on a Mac it checks the ARM64 generator the same way. (The Rust test used 12,017 numbers per divisor.) |
| `records_runtime_calls_and_text_references` | `tests/golden/check/x64__records_runtime_calls_and_text_references.cases` (1 case). The test looked at the generator's list of runtime calls; here the program is built for all three targets and pinned. |

## src/arm64.rs (1 tests)

| Test | Replaced by |
|---|---|
| `encodings_match_the_reference_assembler` | **Not carried over as a table of encodings.** The test compared instruction bytes with a reference assembler. Now every instruction the generator emits for the 105 programs of section `cross` and the 120 of `golden/check` is pinned by hash for all three targets, so no encoding can change unnoticed, and on the machine of the test run the programs are executed, which shows a wrong encoding of that machine. A wrong encoding in new code for another machine is not caught here; the fuzzer (`tests/fuzz`) with an emulator is the tool for that. |

## tests/build_run.rs (6 tests)

| Test | Replaced by |
|---|---|
| `programs_compile_and_run` | `tests/run.py`, section `programs`: every `tests/programs/*.azl` is built and run and compared with its `// expect-exit`, `expect-output`, `expect-line` and `expect-stderr` comments, read by the same rules. |
| `several_files_make_one_program` | Section `programs`: the three orders of `tests/multi/shop`, and the executable named after the first file. |
| `embedded_files_are_found_next_to_their_source` | Section `programs`: `tests/multi/embed` is built from another folder and run there, and the file that is missing gets `E0706` at `1:14`. Section `cross` pins its executables for all three targets. |
| `errors_name_the_file_they_are_in` | Section `programs`: the same six builds of `tests/multi`, each required text, and that nothing is written when a build fails. |
| `check_reports_every_error_and_writes_nothing` | Section `programs`: the same three files through `check`, `check --json`, `build --json`, with the same required texts. |
| `explain_describes_every_code` | Section `errors`, "The error codes". |

## tests/format.rs (8 tests)

| Test | Replaced by |
|---|---|
| `fixtures_format_to_their_expected_text` | Section `format`: every `tests/format/*.input.azl` formats to its `.expected.azl`, which is a fixed point; no expected file without an input; at least 9 pairs. |
| `fmt_rewrites_files_in_place` | Section `format`. |
| `check_lists_unformatted_files_and_changes_nothing` | Section `format`. |
| `stdin_is_formatted_to_stdout` | Section `format`. |
| `fmt_without_files_prints_the_usage` | Section `format`: the same three command lines, exit code 2 and the usage text. |
| `a_file_that_does_not_parse_is_left_untouched` | Section `format`: in place, `--check` and `-`, the parser's error line, the file untouched, the other file of the run formatted. |
| `repository_files_are_already_formatted` | Section `format`: `fmt --check` over `tests/programs`, `tests/multi`, `examples`, `compiler` and, while it exists, `src`; the same five files may differ; exactly one file is refused. |
| `formatting_does_not_change_the_executable` | Section `format`: every test program as written and made ugly by the same rule, built before and after formatting for `aarch64-macos` and `x86_64-linux`; the executables must be the same bytes. |
