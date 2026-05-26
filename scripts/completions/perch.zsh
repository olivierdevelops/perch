#compdef perch
#
# Install:
#   perch --completions zsh > ${fpath[1]}/_perch
# Then restart your shell, or:
#   autoload -Uz compinit && compinit

_perch() {
    local context curcontext="$curcontext" state line
    local -a flags
    flags=(
        '--help[Show summary of commands.perch]'
        '--version[Print perch version]'
        '--init[Write a starter commands.perch]'
        '--shell[Start the REPL]'
        '--server[Serve commands.perch as an HTTP UI]'
        '--build[Bundle commands.perch into a portable binary]'
        '-f[Specify the config file]:file:_files -g "*.perch"'
    )

    if [[ ${words[2]:-} == -* ]]; then
        _describe 'flag' flags
        return
    fi

    # Find -f file if any
    local cfg="commands.perch"
    local i
    for (( i=1; i <= $#words; i++ )); do
        if [[ ${words[$i]} == "-f" && -n ${words[$((i+1))]:-} ]]; then
            cfg=${words[$((i+1))]}
        fi
    done

    local -a cmds
    if [[ -f "$cfg" ]]; then
        cmds=( ${(f)"$(perch --help -f "$cfg" 2>/dev/null | awk '/^  ▸ / { print $2 }')"} )
    fi

    _alternative \
        "commands:command:(${cmds[*]})" \
        'flag:flag:(($flags))'
}

_perch "$@"
