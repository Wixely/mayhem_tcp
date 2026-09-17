param([string]$Directory = '.local/android')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
foreach ($artifact in Get-ChildItem $Directory -File | Where-Object { $_.Extension -in '.apk', '.zip' }) {
    $archive = [IO.Compression.ZipFile]::OpenRead($artifact.FullName)
    try {
        foreach ($entry in $archive.Entries) {
            if ($entry.FullName -match '\.(keystore|jks|pdb)$') { throw "Unexpected private/signing/debug file: $($entry.FullName)" }
            $stream = $entry.Open()
            $memory = New-Object IO.MemoryStream
            try { $stream.CopyTo($memory); $bytes = $memory.ToArray() }
            finally { $stream.Dispose(); $memory.Dispose() }
            foreach ($encoding in @([Text.Encoding]::ASCII, [Text.Encoding]::Unicode)) {
                $text = $encoding.GetString($bytes)
                # Require an actual PEM payload, not crypto-library format strings.
                if ($text -match '(?i)[A-Z]:[\\/]Users[\\/]|gh[pousr]_[A-Za-z0-9]{20}|-----BEGIN [A-Z ]{0,20}PRIVATE KEY-----\s+[A-Za-z0-9+/=]{32}') {
                    throw "Potential sensitive metadata in $($entry.FullName); review before publication"
                }
            }
        }
        Write-Host "$($artifact.Name): reviewed $($archive.Entries.Count) archive entries"
    } finally { $archive.Dispose() }
}
