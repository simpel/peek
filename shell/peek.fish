# peek - inline autocomplete daemon
# Shell integration for fish
# Source via: peek init fish | source

function _peek_ensure_daemon
    peek start &>/dev/null
end

# --- Completions ---
function _peek_complete
    set -l cwd (pwd)
    set -l line (commandline)
    set -l cursor (commandline -C)

    set -l response (peek _suggest --cwd "$cwd" --line "$line" --cursor $cursor 2>/dev/null)
    or return

    set -l names (echo "$response" | grep -o '"name":"[^"]*"' | sed 's/"name":"//;s/"$//')
    set -l previews (echo "$response" | grep -o '"preview":"[^"]*"' | sed 's/"preview":"//;s/"$//')

    set -l i 1
    for name in $names
        set -l preview ""
        if test $i -le (count $previews)
            set preview $previews[$i]
        end
        printf "%s\t%s\n" "$name" "$preview"
        set i (math $i + 1)
    end
end

# Register completions for each tool
complete -c pnpm -f -a '(_peek_complete)'
complete -c npm -f -a '(_peek_complete)'
complete -c yarn -f -a '(_peek_complete)'
complete -c bun -f -a '(_peek_complete)'
complete -c make -f -a '(_peek_complete)'
complete -c cargo -f -a '(_peek_complete)'

# --- Directory Change Hook ---
function _peek_on_pwd --on-variable PWD
    peek _cd --cwd "$PWD" &>/dev/null &
end

# --- Command Tracking ---
function _peek_postexec --on-event fish_postexec
    set -l cmd $argv[1]
    set -l tool_prefixes "pnpm " "pnpm run " "npm run " "yarn " "yarn run " "bun run " "make " "docker compose " "docker-compose " "cargo "
    for p in $tool_prefixes
        if string match -q "$p*" -- "$cmd"
            set -l rest (string replace "$p" "" -- "$cmd")
            set -l command (string split " " -- "$rest")[1]
            set -l tool (string split " " -- "$p")[1]
            peek _executed --cwd "$PWD" --command "$command" --tool "$tool" &>/dev/null &
            break
        end
    end
end

# --- Initialization ---
_peek_ensure_daemon
_peek_on_pwd
