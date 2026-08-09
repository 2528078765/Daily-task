$root = $PSScriptRoot

New-Item -ItemType Directory -Force -Path "$root\dist\vendor" | Out-Null
Copy-Item -LiteralPath "$root\index.html", "$root\styles.css", "$root\app.js" -Destination "$root\dist" -Force
Copy-Item -LiteralPath "$root\vendor\lucide.min.js" -Destination "$root\dist\vendor" -Force

Write-Host "dist synced"
