# Сквозная проверка на настоящей Windows: собрать установочник, поставить,
# запустить, обновить, удалить — и убедиться, что данные пережили всё это.
$ErrorActionPreference = 'Stop'

$bin = (Resolve-Path 'target\release').Path
$work = Join-Path $env:RUNNER_TEMP 'crmedit-e2e'
New-Item -ItemType Directory -Force $work | Out-Null
$env:CRMEDIT_HOME = Join-Path $work 'home'
$name = 'Магазин у дома'

function Check($condition, $what) {
    if (-not $condition) { throw "НЕ ВЫПОЛНЕНО: $what" }
    Write-Host "ok: $what"
}

function Invoke-Editor {
    & "$bin\crmedit-cli.exe" @args
    if ($LASTEXITCODE -ne 0) { throw "crmedit-cli $args -> код $LASTEXITCODE" }
}

# Программа с окном не пишет в консоль, поэтому ждём её и смотрим код возврата.
function Run($exe, $arguments) {
    $process = Start-Process -FilePath $exe -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "$exe $arguments -> код $($process.ExitCode)" }
}

function Wait-Until($what, [scriptblock]$condition) {
    foreach ($attempt in 1..60) {
        if (& $condition) { Write-Host "ok: $what"; return }
        Start-Sleep -Milliseconds 500
    }
    throw "НЕ ДОЖДАЛИСЬ: $what"
}

# Запускает установленную CRM и проверяет, что её окно живёт, а не падает на старте.
function Start-Crm($exe) {
    $process = Start-Process -FilePath $exe -PassThru
    Start-Sleep -Seconds 10
    Check (-not $process.HasExited) 'CRM запущена и работает'
    Stop-Process -Id $process.Id -Force
    $process.WaitForExit()
}

# --- Сборка установочника версии 1 -----------------------------------------
Invoke-Editor new "$work\shop" --name $name
Invoke-Editor build "$work\setup-v1.exe"
Check (Test-Path "$work\setup-v1.exe") 'установочник версии 1 собран'

# --- Установка --------------------------------------------------------------
Run "$work\setup-v1.exe" '--install --silent'
$dir = (Get-ChildItem "$env:LOCALAPPDATA\Programs" -Directory -Filter 'crm_*' | Select-Object -First 1).FullName
Check $dir 'папка программы создана в профиле пользователя'
$id = Split-Path $dir -Leaf
$exe = Join-Path $dir 'crm.exe'
$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$id"
$data = Join-Path $env:APPDATA $id
$links = @('Desktop', 'Programs') | ForEach-Object { Join-Path ([Environment]::GetFolderPath($_)) "$name.lnk" }

Check (Test-Path $exe) 'программа скопирована'
$entry = Get-ItemProperty $key
Check ($entry.DisplayName -eq $name) "в «Программы и компоненты» запись «$($entry.DisplayName)»"
Check ($entry.DisplayVersion -eq '1') 'версия в записи — 1'
Check ($entry.UninstallString -eq "`"$exe`" --uninstall") 'команда удаления указывает на программу'
$shell = New-Object -ComObject WScript.Shell
foreach ($link in $links) {
    Check (Test-Path -LiteralPath $link) "ярлык создан: $link"
    Check ($shell.CreateShortcut($link).TargetPath -eq $exe) 'ярлык ведёт на программу'
}

# --- Первый запуск: создаётся база -----------------------------------------
Start-Crm $exe
$db = Join-Path $data 'crm.db'
Check (Test-Path $db) 'база создана в папке данных, отдельно от программы'
Check (-not (Get-ChildItem $data -Filter '*backup*')) 'у новой базы резервной копии нет'

# --- Обновление: новая версия описания поверх установленной -----------------
Invoke-Editor field add client city 'Город' text
Invoke-Editor build "$work\setup-v2.exe"
Run "$work\setup-v2.exe" '--install --silent'
Check ((Get-ItemProperty $key).DisplayVersion -eq '2') 'после обновления версия в записи — 2'
Check (Test-Path $db) 'обновление не тронуло базу'
Start-Crm $exe
Check (@(Get-ChildItem $data -Filter '*backup*').Count -eq 1) 'перед переносом данных сделана резервная копия'
Start-Crm $exe
Check (@(Get-ChildItem $data -Filter '*backup*').Count -eq 1) 'повторный запуск ничего не переносит'

# --- Удаление: программа уходит, данные остаются ----------------------------
Run $exe '--uninstall --silent'
Wait-Until 'папка программы удалена' { -not (Test-Path $dir) }
Check (-not (Test-Path $key)) 'запись из «Программы и компоненты» удалена'
foreach ($link in $links) { Check (-not (Test-Path -LiteralPath $link)) "ярлык удалён: $link" }
Check (Test-Path $db) 'данные остались на месте'

# --- Повторная установка и удаление вместе с данными ------------------------
Run "$work\setup-v2.exe" '--install --silent'
Run $exe '--uninstall --silent --purge'
Wait-Until 'папка программы удалена' { -not (Test-Path $dir) }
Wait-Until 'данные удалены по явной просьбе' { -not (Test-Path $data) }

Copy-Item "$work\setup-v2.exe" (Join-Path $bin 'Demo-setup.exe')
Write-Host 'Сквозная проверка пройдена.'
