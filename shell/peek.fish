# peek - inline autocomplete daemon
# Shell integration for fish
# Source via: peek init fish | source

function _peek_ensure_daemon
    PEEK_BIN start &>/dev/null
end

# --- Completions ---
function _peek_complete
    set -l line (commandline)
    set -l cursor (commandline -C)

    set -l response (PEEK_BIN _suggest --cwd (pwd) --line "$line" --cursor $cursor 2>/dev/null)
    or return

    # Parse JSON: extract name/preview pairs
    set -l names (string match -r '"name":"([^"]*)"' -a -- "$response" | string replace -r '.*"name":"([^"]*)".*' '$1')
    set -l previews (string match -r '"preview":"([^"]*)"' -a -- "$response" | string replace -r '.*"preview":"([^"]*)".*' '$1')

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

# Erase built-in completions and register ours.
# The -f flag on our registration prevents fish from lazy-loading
# system completions (e.g., /opt/homebrew/share/fish/completions/pnpm.fish).
for tool in pnpm npm yarn bun make cargo
    complete -e -c $tool
    complete -c $tool -f -a '(_peek_complete)'
end

# --- Directory Change Hook ---
function _peek_on_pwd --on-variable PWD
    PEEK_BIN _cd --cwd "$PWD" &>/dev/null &
end

# --- Command Execution Tracking ---
function _peek_postexec --on-event fish_postexec
    set -l cmd $argv[1]
    set -l tool_prefixes "pnpm " "pnpm run " "npm run " "yarn " "yarn run " "bun run " "make " "docker compose " "docker-compose " "cargo "
    for p in $tool_prefixes
        if string match -q "$p*" -- "$cmd"
            set -l rest (string replace "$p" "" -- "$cmd")
            set -l command (string split " " -- "$rest")[1]
            set -l tool (string split " " -- "$p")[1]
            PEEK_BIN _executed --cwd "$PWD" --command "$command" --tool "$tool" &>/dev/null &
            break
        end
    end
end

# --- Initialization ---
_peek_ensure_daemon
_peek_on_pwd
