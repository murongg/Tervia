[[ ${_tervia_integration_loaded-} == 1 ]] && return 0
_tervia_integration_loaded=1
_tervia_hex() { LC_ALL=C builtin printf '%s' "$1" | command od -An -v -tx1 | command tr -d ' \n'; }
_tervia_emit() { builtin printf '\033]777;tervia;%s;%s\007' "$TERVIA_SHELL_TOKEN" "$1"; }
_tervia_preexec() {
    _tervia_ready=0
    _tervia_running=1
    _tervia_emit "C;$(_tervia_hex "$PWD");$(_tervia_hex "$1")"
}
_tervia_before_prompt() {
    local _tervia_status=$?
    _tervia_ready=0
    if [[ $_tervia_running == 1 ]]; then _tervia_emit "D;$_tervia_status"; fi
    _tervia_running=0
    return "$_tervia_status"
}
_tervia_prompt_ready() {
    local _tervia_history=''
    if [[ -n $HISTFILE && ${HISTSIZE:-0} -ne 0 && -o history ]]; then
        case $HISTFILE in /*) _tervia_history=$HISTFILE ;; *) _tervia_history=$PWD/$HISTFILE ;; esac
    fi
    _tervia_emit "H;$(_tervia_hex "$_tervia_history")"
    _tervia_emit "P;$(_tervia_hex "$PWD")"
    _tervia_ready=1
}
if declare -p preexec_functions >/dev/null 2>&1 && declare -p precmd_functions >/dev/null 2>&1; then
    # Reuse bash-preexec when a user's prompt framework already owns the DEBUG trap.
    preexec_functions+=(_tervia_preexec)
    precmd_functions=(_tervia_before_prompt "${precmd_functions[@]}" _tervia_prompt_ready)
elif [[ -z $(trap -p DEBUG) ]]; then
    _tervia_initial_history=$(HISTTIMEFORMAT= builtin history 1)
    _tervia_initial_prompt=1
    _tervia_debug() {
        [[ $_tervia_ready == 1 && $BASH_COMMAND != _tervia_* ]] || return 0
        local _tervia_entry _tervia_input
        _tervia_entry=$(HISTTIMEFORMAT= builtin history 1)
        # Appended first-prompt setup does not add history. Arm only for a new
        # user line; skipping a hidden first line cannot bridge any earlier record.
        if [[ $_tervia_initial_prompt == 1 ]]; then
            [[ $_tervia_entry == "$_tervia_initial_history" ]] && return 0
            _tervia_initial_prompt=0
        fi
        # History prints a status character and one separator after the number.
        # Consume exactly those two characters so the command's own leading spaces survive.
        if [[ $_tervia_entry =~ ^[[:space:]]*([0-9]+)[*\ ][\ ](.*)$ && ${BASH_REMATCH[1]} != "$_tervia_last_history" ]]; then
            _tervia_last_history=${BASH_REMATCH[1]}
            _tervia_input=${BASH_REMATCH[2]}
        else
            # HISTCONTROL/HISTIGNORE may hide this command. Break history learning across it.
            _tervia_input=" $BASH_COMMAND"
        fi
        _tervia_preexec "$_tervia_input"
    }
    if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a '* ]]; then
        PROMPT_COMMAND=(_tervia_before_prompt "${PROMPT_COMMAND[@]}" _tervia_prompt_ready)
    else
        PROMPT_COMMAND="_tervia_before_prompt${PROMPT_COMMAND:+; $PROMPT_COMMAND}; _tervia_prompt_ready"
    fi
    trap '_tervia_debug' DEBUG
fi
