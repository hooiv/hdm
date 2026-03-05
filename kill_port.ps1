$conns = Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue
if ($conns) {
    $procIds = $conns | Select-Object -ExpandProperty OwningProcess -Unique
    foreach ($procId in $procIds) {
        Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
        Write-Host "Killed PID $procId"
    }
} else {
    Write-Host "No process on port 1420"
}
