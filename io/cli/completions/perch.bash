# bash completion for perch
#
# Install:
#   perch --completions bash > /etc/bash_completion.d/perch
# or for a single user:
#   perch --completions bash > ~/.local/share/bash-completion/completions/perch

_perch_complete() {
    local cur prev words cword
    _init_completion 2>/dev/null || COMPREPLY=()

    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Top-level flags
    local flags='--help --version --init --shell --server --build -f'
    if [[ "$cur" == -* ]]; then
        COMPREPLY=( $(compgen -W "$flags" -- "$cur") )
        return 0
    fi

    # -f takes a .perch file
    if [[ "$prev" == "-f" ]]; then
        COMPREPLY=( $(compgen -f -X '!*.perch' -- "$cur") )
        return 0
    fi

    # Otherwise: ask perch for the current command list
    local cfg="commands.perch"
    for i in "${!COMP_WORDS[@]}"; do
        if [[ "${COMP_WORDS[$i]}" == "-f" && -n "${COMP_WORDS[$((i+1))]:-}" ]]; then
            cfg="${COMP_WORDS[$((i+1))]}"
        fi
    done
    if [[ -f "$cfg" ]]; then
        local cmds
        cmds=$(perch --help -f "$cfg" 2>/dev/null | awk '/^  ▸ / { print $2 }')
        COMPREPLY=( $(compgen -W "$cmds $flags" -- "$cur") )
    else
        COMPREPLY=( $(compgen -W "$flags" -- "$cur") )
    fi
}

complete -F _perch_complete perch
