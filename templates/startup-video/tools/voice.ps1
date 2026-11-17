# Offline voice line for the Swofty intro (Windows OneCore TTS, no network).
# powershell -File voice.ps1 -Out voice.wav [-Voice Stefan] [-Text "Swofty"]
param([string]$Out, [string]$Voice = "Stefan", [string]$Text = "Swofty", [string]$Pitch = "-25%", [string]$Rate = "-10%")
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Media.SpeechSynthesis.SpeechSynthesizer, Windows.Media, ContentType = WindowsRuntime]
$null = [Windows.Storage.Streams.DataReader, Windows.Storage.Streams, ContentType = WindowsRuntime]
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
function Await($op, [Type]$t) { $task = $asTask.MakeGenericMethod($t).Invoke($null, @($op)); $task.Wait(-1) | Out-Null; $task.Result }
$s = New-Object Windows.Media.SpeechSynthesis.SpeechSynthesizer
$v = [Windows.Media.SpeechSynthesis.SpeechSynthesizer]::AllVoices | Where-Object { $_.DisplayName -match $Voice } | Select-Object -First 1
if (-not $v) { throw "voice $Voice not found" }
$s.Voice = $v
$lang = $v.Language
$ssml = "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='$lang'><prosody pitch='$Pitch' rate='$Rate'>$Text</prosody></speak>"
$stream = Await ($s.SynthesizeSsmlToStreamAsync($ssml)) ([Windows.Media.SpeechSynthesis.SpeechSynthesisStream])
$size = [uint32]$stream.Size
$reader = New-Object Windows.Storage.Streams.DataReader($stream.GetInputStreamAt(0))
$null = Await ($reader.LoadAsync($size)) ([uint32])
$bytes = New-Object byte[] $size
$reader.ReadBytes($bytes)
[IO.File]::WriteAllBytes($Out, $bytes)
"OK $Out $($v.DisplayName) $size bytes"
