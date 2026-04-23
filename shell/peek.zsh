# peek - inline autocomplete daemon
# Shell integration for zsh
# Source via: eval "$(peek init zsh)"

# --- Configuration ---
PEEK_TRIGGER="${PEEK_TRIGGER:-auto}"
PEEK_MAX_HEIGHT=8

# --- State ---
_peek_suggestions=()
_peek_previews=()
_peek_selected=0
_peek_visible=0
_peek_dropdown_height=0

# --- Communication ---
_peek_suggest() {
  peek _suggest --cwd "$1" --line "$2" --cursor "$3" 2>/dev/null
}

_peek_ensure_daemon() {
  peek start &>/dev/null
}

# --- Dropdown Rendering ---
_peek_clear_dropdown() {
  if (( _peek_visible )); then
    local i
    for (( i = 0; i < _peek_dropdown_height + 2; i++ )); do
      echo -ne "\n\033[2K"
    done
    echo -ne "\033[$(( _peek_dropdown_height + 2 ))A"
    _peek_visible=0
    _peek_dropdown_height=0
  fi
}

_peek_render_dropdown() {
  local count=${#_peek_suggestions[@]}
  if (( count == 0 )); then
    _peek_clear_dropdown
    return
  fi

  local max_height=$PEEK_MAX_HEIGHT
  local height=$(( count < max_height ? count : max_height ))
  _peek_dropdown_height=$height

  local width=50
  echo -ne "\n"
  printf "\033[2K\033[90m┌%${width}s┐\033[0m\n" "" | tr ' ' '─'

  local i start=0
  if (( _peek_selected >= height )); then
    start=$(( _peek_selected - height + 1 ))
  fi

  for (( i = start; i < start + height && i < count; i++ )); do
    local name="${_peek_suggestions[$((i+1))]}"
    local preview="${_peek_previews[$((i+1))]}"

    if (( ${#name} > 20 )); then
      name="${name:0:17}..."
    fi
    if (( ${#preview} > 26 )); then
      preview="${preview:0:23}..."
    fi

    if (( i == _peek_selected )); then
      printf "\033[2K\033[90m│\033[0m \033[7m %-20s \033[2m%-26s \033[0m\033[90m│\033[0m\n" "$name" "$preview"
    else
      printf "\033[2K\033[90m│\033[0m  %-20s \033[2m%-26s\033[0m \033[90m│\033[0m\n" "$name" "$preview"
    fi
  done

  printf "\033[2K\033[90m└%${width}s┘\033[0m" "" | tr ' ' '─'
  echo -ne "\033[$(( height + 2 ))A"

  _peek_visible=1
}

# --- Suggestion Fetching ---
_peek_fetch_suggestions() {
  local response
  response=$(_peek_suggest "$PWD" "$BUFFER" "$CURSOR") || return

  _peek_suggestions=()
  _peek_previews=()
  _peek_selected=0

  local names=($(echo "$response" | grep -o '"name":"[^"]*"' | sed 's/"name":"//;s/"$//'))
  local previews=($(echo "$response" | grep -o '"preview":"[^"]*"' | sed 's/"preview":"//;s/"$//'))

  local i
  for (( i = 1; i <= ${#names[@]}; i++ )); do
    _peek_suggestions+=("${names[$i]}")
    _peek_previews+=("${(Q)${previews[$i]:-}}")
  done
}

# --- Key Handlers ---
_peek_accept() {
  if (( _peek_visible && ${#_peek_suggestions[@]} > 0 )); then
    local selected="${_peek_suggestions[$(( _peek_selected + 1 ))]}"
    _peek_clear_dropdown

    local prefix=""
    local tool_prefixes=("pnpm run " "pnpm " "npm run " "yarn run " "yarn " "bun run " "make " "docker compose " "docker-compose " "cargo ")
    for p in "${tool_prefixes[@]}"; do
      if [[ "$BUFFER" == "$p"* ]]; then
        prefix="$p"
        break
      fi
    done

    if [[ -n "$prefix" ]]; then
      BUFFER="${prefix}${selected}"
      CURSOR=${#BUFFER}
    fi
  fi
  zle redisplay
}

_peek_accept_and_run() {
  _peek_accept
  _peek_clear_dropdown
  zle accept-line
}

_peek_dismiss() {
  _peek_clear_dropdown
  zle redisplay
}

_peek_up() {
  if (( _peek_visible )); then
    if (( _peek_selected > 0 )); then
      (( _peek_selected-- ))
    else
      _peek_selected=$(( ${#_peek_suggestions[@]} - 1 ))
    fi
    _peek_clear_dropdown
    _peek_render_dropdown
    zle redisplay
  else
    zle up-line-or-history
  fi
}

_peek_down() {
  if (( _peek_visible )); then
    if (( _peek_selected < ${#_peek_suggestions[@]} - 1 )); then
      (( _peek_selected++ ))
    else
      _peek_selected=0
    fi
    _peek_clear_dropdown
    _peek_render_dropdown
    zle redisplay
  else
    zle down-line-or-history
  fi
}

_peek_self_insert() {
  zle .self-insert
  _peek_maybe_suggest
}

_peek_backward_delete_char() {
  zle .backward-delete-char
  if (( _peek_visible )); then
    _peek_maybe_suggest
  fi
}

_peek_maybe_suggest() {
  local should_suggest=0
  local tool_prefixes=("pnpm " "pnpm run " "npm run " "yarn " "yarn run " "bun run " "make " "docker compose " "docker-compose " "cargo ")
  for p in "${tool_prefixes[@]}"; do
    if [[ "$BUFFER" == "$p"* ]]; then
      should_suggest=1
      break
    fi
  done

  if (( should_suggest )); then
    _peek_fetch_suggestions
    _peek_clear_dropdown
    if (( ${#_peek_suggestions[@]} > 0 )); then
      _peek_render_dropdown
    fi
  else
    _peek_clear_dropdown
  fi
}

# --- ZLE Widgets ---
zle -N _peek_accept
zle -N _peek_accept_and_run
zle -N _peek_dismiss
zle -N _peek_up
zle -N _peek_down
zle -N _peek_self_insert
zle -N _peek_backward_delete_char

# --- Keybindings ---
if [[ "$PEEK_TRIGGER" == "auto" ]]; then
  bindkey -M main '\t' _peek_accept
  bindkey -M main '\r' _peek_accept_and_run
  bindkey -M main '\e' _peek_dismiss
  bindkey -M main '\e[A' _peek_up
  bindkey -M main '\e[B' _peek_down
  bindkey -M main '^?' _peek_backward_delete_char

  local key
  for key in {a..z} {A..Z} {0..9} '-' '_' ':' '.' '/'; do
    bindkey -M main "$key" _peek_self_insert
  done
fi

# --- Directory Change Hook ---
_peek_chpwd() {
  peek _cd --cwd "$PWD" &>/dev/null &
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
      peek _executed --cwd "$PWD" --command "$command" --tool "$tool" &>/dev/null &
      break
    fi
  done
}
add-zsh-hook preexec _peek_preexec

# --- Initialization ---
_peek_ensure_daemon
_peek_chpwd
