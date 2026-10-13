# Local tool environment

Copy `environment.example.psd1` to `environment.local.psd1`, then fill in the
executable paths you want to override. The local file is ignored by Git; the
example contains no machine-specific paths. Use a standalone Python installation.
Bash is optional; its configured path selects Git Bash, MSYS or WSL explicitly.

For the current PowerShell session:

```powershell
. ./build/Use-Environment.ps1
python -m unittest discover -s tools/tests
python tools/corridor-scenery.py status local/oc_to_solana/full-corridor
./tools/build.ps1 -Release -Offline
bash path/to/script.sh
```

For a single command in a fresh shell:

```powershell
./build/run.ps1 python tools/corridor-scenery.py status local/oc_to_solana/full-corridor
./build/run.ps1 powershell -NoProfile -File tools/build.ps1 -Release -Offline
```

The setup validates overrides before applying them, adds session aliases and
executable directories to this process's PATH, and exports `OPENRIDE_PYTHON`,
`OPENRIDE_GODOT`, `OPENRIDE_POWERSHELL` and `OPENRIDE_BASH` for child processes.
Empty overrides discover applications on PATH, excluding WindowsApps launchers
and known bundled Python locations. Missing optional tools are left unset.
Build/export scripts honor `OPENRIDE_GODOT`; explicit `-Godot` still takes priority.

Close the shell to discard this setup. It does not edit shell profiles, permanent
environment variables, execution policies or sandbox permissions. Bash scripts
still follow the path conventions of the selected Bash distribution.
