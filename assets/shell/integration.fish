function _tervia_hex
    printf '%s' "$argv[1]" | command od -An -v -tx1 | command tr -d ' \n'
end
function _tervia_emit
    printf '\e]777;tervia;%s;%s\a' "$TERVIA_SHELL_TOKEN" "$argv[1]"
end
function _tervia_preexec --on-event fish_preexec
    set -g _tervia_running 1
    _tervia_emit "C;"(_tervia_hex "$PWD")";"(_tervia_hex "$argv[1]")
end
function _tervia_postexec --on-event fish_postexec
    set -l _tervia_status $status
    _tervia_emit "D;$_tervia_status"
    set -g _tervia_running 0
end
function _tervia_prompt --on-event fish_prompt
    set -l _tervia_name fish
    if set -q fish_history
        set _tervia_name "$fish_history"
    end
    set -l _tervia_history ''
    if test -n "$_tervia_name"; and test -z "$fish_private_mode"
        set -l _tervia_data "$HOME/.local/share"
        if test -n "$XDG_DATA_HOME"
            set _tervia_data "$XDG_DATA_HOME"
        end
        if not string match -q '/*' -- "$_tervia_data"
            set _tervia_data "$PWD/$_tervia_data"
        end
        set _tervia_history "$_tervia_data/fish/$_tervia_name"_history
    end
    _tervia_emit "H;"(_tervia_hex "$_tervia_history")
    _tervia_emit "P;"(_tervia_hex "$PWD")
end
