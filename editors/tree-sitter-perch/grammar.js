// Tree-sitter grammar for perch's command-file DSL.
//
// Models the indent-significant surface as the perch loader sees it:
//   - top-level statements: name / about / version / globals / command / catch
//   - command body: config statements before `do`; ops inside `do … end`
//   - block ops (if_*, for_each) nest further bodies terminated by `end`
//
// Generate the parser with `tree-sitter generate` from this folder.

const sep1 = (rule, sep) => seq(rule, repeat(seq(sep, rule)));

module.exports = grammar({
  name: 'perch',

  extras: $ => [/\s+/, $.comment],

  conflicts: $ => [],

  rules: {
    source_file: $ => repeat($._top_level),

    _top_level: $ => choice(
      $.name_decl,
      $.about_decl,
      $.version_decl,
      $.globals_block,
      $.command_block,
      $.catch_block,
    ),

    // ── top-level metadata ──────────────────────────────────────────────

    name_decl:    $ => seq('name', $.string),
    about_decl:   $ => seq('about', $.string),
    version_decl: $ => seq('version', $.string),

    // ── globals ─────────────────────────────────────────────────────────

    globals_block: $ => seq(
      'globals',
      repeat($.global_assign),
      'end',
    ),

    global_assign: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._literal),
    ),

    // ── command + catch ─────────────────────────────────────────────────

    command_block: $ => seq(
      'command',
      field('name', $.identifier),
      repeat($._config_stmt),
      optional($.do_block),
      'end',
    ),

    catch_block: $ => seq(
      'catch',
      field('bind', $.identifier),
      repeat($._config_stmt),
      optional($.do_block),
      'end',
    ),

    // ── config statements (before `do`) ─────────────────────────────────

    _config_stmt: $ => choice(
      $.description_stmt,
      $.arg_stmt,
      $.arg_default_stmt,
      $.arg_index_stmt,
      $.arg_optional_stmt,
      $.private_stmt,
      $.detached_stmt,
      $.proxy_args_stmt,
      $.require_os_stmt,
      $.require_arch_stmt,
      $.dir_stmt,
      $.on_signal_stmt,
      $.env_stmt,
    ),

    description_stmt:  $ => seq('description', $.string),
    arg_stmt:          $ => seq('arg', $.identifier, $.identifier, $.string),
    arg_default_stmt:  $ => seq('arg_default', $.identifier, $._literal),
    arg_index_stmt:    $ => seq('arg_index', $.identifier, $.integer),
    arg_optional_stmt: $ => seq('arg_optional', $.identifier),
    private_stmt:      $ => 'private',
    detached_stmt:     $ => 'detached',
    proxy_args_stmt:   $ => 'proxy_args',
    require_os_stmt:   $ => seq('require_os',   sep1($.string, /[ \t]+/)),
    require_arch_stmt: $ => seq('require_arch', sep1($.string, /[ \t]+/)),
    dir_stmt:          $ => seq('dir', $.string),
    on_signal_stmt:    $ => seq('on_signal', $.identifier),
    env_stmt:          $ => seq('env', $.identifier, $.string),

    // ── do { ops } ──────────────────────────────────────────────────────

    do_block: $ => seq(
      'do',
      repeat($._op),
      'end',
    ),

    _op: $ => choice(
      $.let_stmt,
      $.block_op,
      $.call_op,
    ),

    let_stmt: $ => seq(
      'let',
      field('name', $.identifier),
      '=',
      field('callee', $.identifier),
      repeat($._arg_value),
    ),

    block_op: $ => seq(
      field('opener', $._block_op_name),
      repeat($._arg_value),
      repeat($._op),
      'end',
    ),

    _block_op_name: $ => choice(
      'if_os', 'if_arch', 'if_exists',
      'if_eq', 'if_neq', 'if_gt', 'if_lt',
      'if_empty', 'if_not_empty',
      'for_each',
    ),

    call_op: $ => seq(
      field('callee', $.identifier),
      repeat($._arg_value),
    ),

    _arg_value: $ => choice($.string, $.integer, $.float, $.boolean, $.null, $.identifier),

    // ── literals ────────────────────────────────────────────────────────

    _literal: $ => choice($.string, $.integer, $.float, $.boolean, $.null),

    string: $ => choice(
      seq('"', repeat(choice($.interpolation, $._string_char_dq)), '"'),
      seq("'", repeat(choice($.interpolation, $._string_char_sq)), "'"),
    ),

    _string_char_dq: $ => choice(
      /[^"\\$]+/,
      /\\./,
      /\$[^{]/,
    ),
    _string_char_sq: $ => choice(
      /[^'\\$]+/,
      /\\./,
      /\$[^{]/,
    ),

    interpolation: $ => seq('${', $.identifier, '}'),

    integer:    $ => /-?\d+/,
    float:      $ => /-?\d+\.\d+/,
    boolean:    $ => choice('true', 'false'),
    null:       $ => 'null',
    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    comment: $ => token(seq('#', /.*/)),
  },
});
