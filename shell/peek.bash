# peek - inline autocomplete daemon
# Shell integration for bash
# Source via: eval "$(peek init bash)"

# --- Completion Function ---
_peek_complete() {
  local line="${COMP_LINE}"
  local cursor="${COMP_POINT}"

  local response
  response=$(PEEK_BIN _suggest --cwd "$PWD" --line "$line" --cursor "$cursor" 2>/dev/null) || return

  local names
  names=$(echo "$response" | grep -o '"name":"[^"]*"' | sed 's/"name":"//;s/"$//')

  COMPREPLY=()
  while IFS= read -r name; do
    [[ -n "$name" ]] && COMPREPLY+=("$name")
  done <<< "$names"
}

# Register completions for managed tools
complete -o default -F _peek_complete pnpm
complete -o default -F _peek_complete npm
complete -o default -F _peek_complete yarn
complete -o default -F _peek_complete bun
complete -o default -F _peek_complete make
complete -o default -F _peek_complete cargo

# --- Daemon ---
_peek_ensure_daemon() {
  PEEK_BIN start &>/dev/null
}

# --- Directory Change Tracking ---
_peek_prompt_command() {
  if [[ "$PWD" != "$_PEEK_LAST_DIR" ]]; then
    _PEEK_LAST_DIR="$PWD"
    PEEK_BIN _cd --cwd "$PWD" &>/dev/null &
  fi
}
_PEEK_LAST_DIR=""

if [[ -z "$PROMPT_COMMAND" ]]; then
  PROMPT_COMMAND="_peek_prompt_command"
else
  PROMPT_COMMAND="_peek_prompt_command;$PROMPT_COMMAND"
fi

# --- Command Tracking ---
_peek_preexec_trap() {
  local cmd="$BASH_COMMAND"
  local tool_prefixes=("pnpm " "pnpm run " "npm run " "yarn " "yarn run " "bun run " "make " "docker compose " "docker-compose " "cargo ")
  for p in "${tool_prefixes[@]}"; do
    if [[ "$cmd" == "$p"* ]]; then
      local rest="${cmd#$p}"
      local command="${rest%% *}"
      local tool="${p%% *}"
      PEEK_BIN _executed --cwd "$PWD" --command "$command" --tool "$tool" &>/dev/null &
      break
    fi
  done
}
trap '_peek_preexec_trap' DEBUG

# --- Initialization ---
_peek_ensure_daemon
_peek_prompt_command
