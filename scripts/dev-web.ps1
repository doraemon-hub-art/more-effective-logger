<#
@file dev-web.ps1
@brief Open the dev server of a remote host in this machine's browser
@author doraemon-hub-art <1660219734@qq.com>
@date 2026-10-23
@copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.

Runs on the machine with the browser, not on the dev host. The dev server listens on
127.0.0.1 there, so it is reached through an ssh local forward and the browser opens on
localhost here.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Server,
    [string]$User = "dev",
    [int]$SshPort = 22,
    [int]$RemotePort = 1420,
    [int]$LocalPort = 1420
)

function Test-LocalPort {
    param([int]$Port)
    $client = New-Object System.Net.Sockets.TcpClient
    try {
        $client.Connect('127.0.0.1', $Port)
        return $true
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
}

if (-not (Get-Command ssh -ErrorAction SilentlyContinue)) {
    Write-Error "ssh is not on PATH"
    exit 1
}

$url = "http://localhost:$LocalPort/"

if (Test-LocalPort -Port $LocalPort) {
    Write-Host "localhost:$LocalPort is already listening, opening $url"
    Start-Process $url
    exit 0
}

$sshArgs = @(
    '-N',
    '-o', 'ExitOnForwardFailure=yes',
    '-L', "${LocalPort}:127.0.0.1:${RemotePort}",
    '-p', "$SshPort",
    "$User@$Server"
)

Write-Host "forwarding localhost:$LocalPort to ${Server}:$RemotePort over ${User}@${Server}:$SshPort"
$tunnel = Start-Process ssh -ArgumentList $sshArgs -PassThru -NoNewWindow

$up = $false
for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep -Milliseconds 500
    if ($tunnel.HasExited) {
        Write-Error "ssh exited with code $($tunnel.ExitCode)"
        exit 1
    }
    if (Test-LocalPort -Port $LocalPort) {
        $up = $true
        break
    }
}

if (-not $up) {
    Write-Error "port $LocalPort did not come up"
    Stop-Process -Id $tunnel.Id -Force -ErrorAction SilentlyContinue
    exit 1
}

Start-Process $url
Write-Host "opened $url"
Write-Host "tunnel pid $($tunnel.Id), stop with: Stop-Process -Id $($tunnel.Id)"
