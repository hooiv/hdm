# Generate simple WAV files for audio events

function New-SimpleWav {
    param(
        [string]$FilePath,
        [int]$Frequency = 440,
        [int]$DurationMs = 200
    )
    
    $sampleRate = 44100
    $numSamples = [int](($sampleRate * $DurationMs) / 1000)
    
    # Create byte array for WAV file
    $wav = New-Object System.Collections.Generic.List[byte]
    
    # RIFF header
    $wav.AddRange([System.Text.Encoding]::ASCII.GetBytes("RIFF"))
    $fileSize = 36 + ($numSamples * 2)
    $wav.AddRange([BitConverter]::GetBytes([uint32]$fileSize))
    $wav.AddRange([System.Text.Encoding]::ASCII.GetBytes("WAVE"))
    
    # fmt chunk
    $wav.AddRange([System.Text.Encoding]::ASCII.GetBytes("fmt "))
    $wav.AddRange([BitConverter]::GetBytes([uint32]16))  # chunk size
    $wav.AddRange([BitConverter]::GetBytes([uint16]1))   # PCM format
    $wav.AddRange([BitConverter]::GetBytes([uint16]1))   # mono
    $wav.AddRange([BitConverter]::GetBytes([uint32]$sampleRate))
    $byteRate = $sampleRate * 2  # 16-bit mono
    $wav.AddRange([BitConverter]::GetBytes([uint32]$byteRate))
    $wav.AddRange([BitConverter]::GetBytes([uint16]2))   # block align
    $wav.AddRange([BitConverter]::GetBytes([uint16]16))  # bits per sample
    
    # data chunk
    $wav.AddRange([System.Text.Encoding]::ASCII.GetBytes("data"))
    $dataSize = $numSamples * 2
    $wav.AddRange([BitConverter]::GetBytes([uint32]$dataSize))
    
    # Generate audio samples (sine wave)
    for ($i = 0; $i -lt $numSamples; $i++) {
        $t = $i / $sampleRate
        $sample = [Math]::Sin($t * $Frequency * 2 * [Math]::PI)
        
        # Fade out at the end
        $fadeSamples = $sampleRate / 10
        if ($i -gt ($numSamples - $fadeSamples)) {
            $fadeFactor = ($numSamples - $i) / $fadeSamples
            $sample *= $fadeFactor
        }
        
        $amplitude = [int16]($sample * 32767 * 0.3)  # 30% volume
        $wav.AddRange([BitConverter]::GetBytes($amplitude))
    }
    
    [System.IO.File]::WriteAllBytes($FilePath, $wav.ToArray())
}

Write-Host "Generating sound files..."

# Create directory if it doesn't exist
$soundDir = "assets\sounds"
if (-not (Test-Path $soundDir)) {
    New-Item -ItemType Directory -Path $soundDir -Force | Out-Null
}

# Generate sounds
New-SimpleWav -FilePath "$soundDir\success.wav" -Frequency 660 -DurationMs 200
Write-Host "Created success.wav"

New-SimpleWav -FilePath "$soundDir\error.wav" -Frequency 330 -DurationMs 300
Write-Host "Created error.wav"

New-SimpleWav -FilePath "$soundDir\start.wav" -Frequency 523 -DurationMs 100
Write-Host "Created start.wav"

Write-Host "Done!"
