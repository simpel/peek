# peek - inline autocomplete daemon
# Shell integration for bash
# Source via: eval "$(peek init bash)"

# --- Configuration ---
PEEK_SOCKET="${HOME}/.peek/peek.sock"
PEEK_TRIGGER="${PEEK_TRIGGER:-auto}"

# --- Communication ---
_peek_query() {
  local request="$1"
  if [[ ! -S "$PEEK_SOCKET" ]]; then
    return 1
  fi
  echo "$request" | socat - UNIX-CONNECT:"$PEEK_SOCKET" 2>/dev/null
}

_peek_ensure_daemon() {
  if [[ ! -S "$PEEK_SOCKET" ]]; then
    peekd &>/dev/null &
    disown
    local i=0
    while [[ ! -S "$PEEK_SOCKET" ]] && (( i < 10 )); do
      sleep 0.05
      (( i++ ))
    done
  fi
}

# --- Completion Function ---
_peek_complete() {
  local cwd="$PWD"
  local line="${COMP_LINE}"
  local cursor="${COMP_POINT}"

  local request="{\"type\":\"suggest\",\"cwd\":\"$cwd\",\"line\":\"$line\",\"cursor\":$cursor}"
  local response
  response=$(_peek_query "$request") || return

  # Extract suggestion names
  local names
  names=$(echo "$response" | grep -o '"name":"[^"]*"' | sed 's/"name":"//;s/"$//')

  COMPREPLY=()
  while IFS= read -r name; do
    [[ -n "$name" ]] && COMPREPLY+=("$name")
  done <<< "$names"
}

# Register completions for tool commands
complete -F _peek_complete pnpm
complete -F _peek_complete npm
complete -F _peek_complete yarn
complete -F _peek_complete bun
complete -F _peek_complete make
complete -F _peek_complete cargo

# --- Directory Change Tracking ---
_peek_prompt_command() {
  local cwd="$PWD"
  if [[ "$cwd" != "$_PEEK_LAST_DIR" ]]; then
    _PEEK_LAST_DIR="$cwd"
    _peek_query "{\"type\":\"cd\",\"cwd\":\"$cwd\"}" &>/dev/null &
  fi
}
_PEEK_LAST_DIR=""

if [[ -z "$PROMPT_COMMAND" ]]; then
  PROMPT_COMMAND="_peek_prompt_command"
else
  PROMPT_COMMAND="_peek_prompt_command;$PROMPT_COMMAND"
fi

# --- Command Execution Tracking ---
_peek_debug_trap() {
  local cmd="$BASH_COMMAND"
  local tool_prefixes=("pnpm " "pnpm run " "npm run " "yarn " "yarn run " "bun run " "make " "docker compose " "docker-compose " "cargo ")
  for p in "${tool_prefixes[@]}"; do
    if [[ "$cmd" == "$p"* ]]; then
      local rest="${cmd#$p}"
      local command="${rest%% *}"
      local tool="${p%% *}"
      _peek_query "{\"type\":\"executed\",\"cwd\":\"$PWD\",\"command\":\"$command\",\"tool\":\"$tool\"}" &>/dev/null &
      break
    fi
  done
}
trap '_peek_debug_trap' DEBUG

# --- Initialization ---
_peek_ensure_daemon
_peek_prompt_command
