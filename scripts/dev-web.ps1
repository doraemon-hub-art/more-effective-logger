<#
@file dev-web.ps1
@brief Open the dev server of a remote host in this machine's browser
@author doraemon-hub-art <1660219734@qq.com>
@date 2026-10-23
@copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.

Runs on the machine with the browser, not on the dev host. The dev server and the backend
bridge both listen on 127.0.0.1 there, so they are reached through ssh local forwards and
the browser opens on localhost here. The tunnel stays in the foreground: Ctrl+C ends it and
takes the ssh process with it.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Server,
    [string]$User = "dev",
    [int]$SshPort = 22,
    [int]$RemotePort = 1420,
    [int]$LocalPort = 1420,
    [int]$BridgePort = 1421
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
    if (Test-LocalPort -Port $BridgePort) {
        Write-Host "localhost:$LocalPort and localhost:$BridgePort are already listening, opening $url"
        Start-Process $url
        exit 0
    }
    Write-Warning "localhost:$LocalPort is taken but localhost:$BridgePort is not: a tunnel without the bridge is still running. End it and run again."
    exit 1
}

$sshArgs = @(
    '-N',
    '-o', 'ExitOnForwardFailure=yes',
    '-L', "${LocalPort}:127.0.0.1:${RemotePort}",
    '-L', "${BridgePort}:127.0.0.1:${BridgePort}",
    '-p', "$SshPort",
    "$User@$Server"
)

Write-Host "forwarding localhost:$LocalPort to ${Server}:$RemotePort and localhost:$BridgePort to ${Server}:$BridgePort over ${User}@${Server}:$SshPort"
$tunnel = Start-Process ssh -ArgumentList $sshArgs -PassThru -NoNewWindow

try {
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
        exit 1
    }

    Start-Process $url
    Write-Host "opened $url"
    Write-Host "tunnel pid $($tunnel.Id), Ctrl+C to end it"

    # Nothing else to do but stay: the script lives as long as the tunnel does.
    $tunnel.WaitForExit()
    Write-Host "tunnel closed"
} finally {
    if (-not $tunnel.HasExited) {
        Stop-Process -Id $tunnel.Id -Force -ErrorAction SilentlyContinue
    }
}
