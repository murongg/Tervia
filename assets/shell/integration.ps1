# Keep the existing prompt and PSReadLine history policy. No profile files are changed.
if (-not (Get-Module PSReadLine)) { Import-Module PSReadLine -ErrorAction SilentlyContinue }
if (Get-Command Set-PSReadLineOption -ErrorAction SilentlyContinue) {
    function global:_tervia_hex([string]$value) {
        [BitConverter]::ToString([Text.Encoding]::UTF8.GetBytes($value)).Replace('-', '')
    }
    function global:_tervia_emit([string]$value) {
        [Console]::Write("$([char]27)]777;tervia;$env:TERVIA_SHELL_TOKEN;$value$([char]7)")
    }
    $global:_tervia_original_prompt = (Get-Item Function:\prompt).ScriptBlock
    $global:_tervia_history_handler = (Get-PSReadLineOption).AddToHistoryHandler
    Set-PSReadLineOption -AddToHistoryHandler {
        param($line)
        $decision = $true
        if ($global:_tervia_history_handler) { $decision = $global:_tervia_history_handler.Invoke($line) }
        $inputLine = $line
        if (($decision -is [bool] -and -not $decision) -or "$decision" -in @('SkipAdding', 'MemoryOnly')) {
            $inputLine = ' ' + $line
        }
        $global:_tervia_running = $true
        _tervia_emit "C;$(_tervia_hex $PWD.ProviderPath);$(_tervia_hex $inputLine)"
        return $decision
    }
    function global:prompt {
        $succeeded = $?
        $code = 0
        if (-not $succeeded) { $code = 1; if ($global:LASTEXITCODE) { $code = $global:LASTEXITCODE } }
        if ($global:_tervia_running) { _tervia_emit "D;$code" }
        $global:_tervia_running = $false
        $historyOptions = Get-PSReadLineOption
        $historyPath = ''
        if ($historyOptions.HistorySaveStyle -ne 'SaveNothing') {
            $historyPath = $historyOptions.HistorySavePath
            if ($historyPath -and -not [IO.Path]::IsPathRooted($historyPath)) {
                $historyPath = Join-Path $PWD.ProviderPath $historyPath
            }
        }
        _tervia_emit "H;$(_tervia_hex $historyPath)"
        _tervia_emit "P;$(_tervia_hex $PWD.ProviderPath)"
        & $global:_tervia_original_prompt
    }
}
