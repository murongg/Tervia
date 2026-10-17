# Installed after the user's RC files; function arrays preserve prompt themes and plugins.
_tervia_hex() { LC_ALL=C builtin printf '%s' "$1" | command od -An -v -tx1 | command tr -d ' \n'; }
_tervia_emit() { builtin printf '\033]777;tervia;%s;%s\007' "$TERVIA_SHELL_TOKEN" "$1"; }
_tervia_preexec() {
    _tervia_running=1
    _tervia_emit "C;$(_tervia_hex "$PWD");$(_tervia_hex "$1")"
}
_tervia_precmd() {
    local _tervia_status=$?
    if [[ $_tervia_running == 1 ]]; then _tervia_emit "D;$_tervia_status"; fi
    _tervia_running=0
    local _tervia_history=''
    if [[ -n $HISTFILE && ${SAVEHIST:-0} -gt 0 ]]; then _tervia_history=${HISTFILE:A}; fi
    _tervia_emit "H;$(_tervia_hex "$_tervia_history")"
    _tervia_emit "P;$(_tervia_hex "$PWD")"
    return $_tervia_status
}
typeset -ga preexec_functions precmd_functions
preexec_functions=(${preexec_functions:#_tervia_preexec} _tervia_preexec)
precmd_functions=(_tervia_precmd ${precmd_functions:#_tervia_precmd})
