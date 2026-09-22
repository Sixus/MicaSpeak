$states = @('connect','main','main-dark','speaking','settings','overlay-speaking','disconnected','error')
foreach ($state in $states) {
  Get-Process micaspeak -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Milliseconds 350
  if ($state -eq 'connect') {
    Remove-Item Env:MICASPEAK_UI_STATE -ErrorAction SilentlyContinue
  } else {
    $env:MICASPEAK_UI_STATE = $state
  }
  Start-Process -FilePath 'D:\MicaSpeak\target\debug\micaspeak.exe' -WorkingDirectory 'D:\MicaSpeak'
  Start-Sleep -Milliseconds 1300
  $dir = "D:\MicaSpeak\logs\acceptance-capture\$state"
  powershell -ExecutionPolicy Bypass -File 'D:\MicaSpeak\logs\printwindow-followup.ps1' -OutDir $dir | Write-Output
}
Get-Process micaspeak -ErrorAction SilentlyContinue | Stop-Process -Force
