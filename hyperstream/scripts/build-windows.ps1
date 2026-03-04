# PowerShell helper for Windows developers
# sets the required CMAKE_GENERATOR and runs a full Tauri build

param(
    [switch]$Dev  # if set, runs `npm run tauri -- dev` instead of build
)

# ensure we are in the hyperstream folder
Push-Location -Path "$PSScriptRoot/.."

# make sure dependencies are installed
npm install

# set generator
$env:CMAKE_GENERATOR = "Ninja"

if ($Dev) {
    npm run tauri -- dev
} else {
    npm run tauri -- build
}

Pop-Location
