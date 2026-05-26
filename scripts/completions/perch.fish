# Fish completion for perch
#
# Install:
#   perch --completions fish > ~/.config/fish/completions/perch.fish

function __perch_commands
    set -l cfg commands.capy
    set -l args $argv
    for i in (seq (count $args))
        if test $args[$i] = -f
            set cfg $args[(math $i + 1)]
        end
    end
    if test -f $cfg
        perch --help -f $cfg 2>/dev/null | string match -r '^  ▸ (\\S+)' | string replace -r '^  ▸ ' ''
    end
end

complete -c perch -f

# Flags
complete -c perch -l help    -d "Show summary of commands.capy"
complete -c perch -l version -d "Print perch version"
complete -c perch -l init    -d "Write a starter commands.capy"
complete -c perch -l shell   -d "Start the REPL"
complete -c perch -l server  -d "Serve commands.capy as an HTTP UI"
complete -c perch -l build   -d "Bundle commands.capy into a portable binary"
complete -c perch -s f       -d "Specify the config file" -r -F

# Commands (read from commands.capy)
complete -c perch -n "not __fish_seen_subcommand_from -- (perch --help 2>/dev/null | string match -r '  ▸ \\S+' | string trim)" \
    -a "(__perch_commands (commandline -opc))"
