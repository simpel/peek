# peek - inline autocomplete daemon
# Shell integration for zsh
# Source via: eval "$(peek init zsh)"

# --- Completion Function ---
_peek_complete() {
  local line="$words"
  local response
  response=$(PEEK_BIN _suggest --cwd "$PWD" --line "$line" --cursor $CURRENT 2>/dev/null) || return

  local -a completions
  local names=($(echo "$response" | grep -o '"name":"[^"]*"' | sed 's/"name":"//;s/"$//'))
  local previews=($(echo "$response" | grep -o '"preview":"[^"]*"' | sed 's/"preview":"//;s/"$//'))

  local i
  for (( i = 1; i <= ${#names[@]}; i++ )); do
    local name="${names[$i]}"
    local preview="${previews[$i]:-}"
    if [[ -n "$preview" ]]; then
      completions+=("${name}:${preview}")
    else
      completions+=("${name}")
    fi
  done

  _describe 'script' completions
}

# Register completions for managed tools
compdef _peek_complete pnpm
compdef _peek_complete npm
compdef _peek_complete yarn
compdef _peek_complete bun
compdef _peek_complete make
compdef _peek_complete cargo

# --- Daemon ---
_peek_ensure_daemon() {
  PEEK_BIN start &>/dev/null
}

# --- Directory Change Hook ---
_peek_chpwd() {
  PEEK_BIN _cd --cwd "$PWD" &>/dev/null &
}
autoload -Uz add-zsh-hook
add-zsh-hook chpwd _peek_chpwd

# --- Command Tracking ---
_peek_preexec() {
  local cmd="$1"
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
add-zsh-hook preexec _peek_preexec

# --- Initialization ---
_peek_ensure_daemon
_peek_chpwd
