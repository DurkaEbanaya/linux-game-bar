# Linux Game Bar

Нативная игровая панель для **KDE Plasma Wayland, Hyprland и Sway**:
Rust, Smithay Client Toolkit, egui 0.36 и wgpu 30. Дизайн адаптирован из
`rust-gui-silver-bullet`: Pro Dark / Studio Light / Modern Dark, Painter-иконки,
локальные шрифты с кириллицей.

**Статус: ранняя собираемая реализация, не проверенный игровой релиз.**
Сборка проходит на openSUSE Tumbleweed. По просьбе автора тесты и игровые проверки
пока не запускались. [Точный статус](verification/RESULTS.md).

## Возможности

- Полноэкранная прозрачная Wayland **overlay layer**. При скрытии surface и GPU
  presentation уничтожаются; виджетов поверх игры не остаётся.
- **Win+G в KDE** через автоматически загружаемый KWin 6 script. Esc закрывает панель.
  В Hyprland/Sway используется compositor key binding на `gamebar toggle`.
- Панель перетаскивается за заголовок, меняет размер за края; slider «Масштаб»
  меняет размер интерфейса. Геометрия сохраняется между вызовами Win+G текущего запуска.
- Собственный Wayland cursor устанавливается сразу при pointer enter; resize/drag
  cursors синхронизируются с egui, не наследуя скрытый курсор игры.
- Выбор процесса исходного активного окна и монитора перед захватом фокуса панелью.
- Native PipeWire-мониторинг потоков: громкость, отдельное отключение **звука приложения**
  и **его входящего потока микрофона**, без глобального выключения устройства.
  Политики действуют на новые потоки приложения в течение текущего запуска панели.
- `/proc` CPU, RSS RAM, I/O; DRM fdinfo GPU/VRAM по приложению; sysfs/hwmon
  системные GPU, VRAM и температуры. Недоступные данные обозначены `—`.
- Список приложений, сортировка CPU/RAM/GPU/I/O, поиск, закрытие исходного окна,
  SIGTERM/SIGKILL через pidfd с проверкой starttime.
- Экспериментальный FPS hot-attach helper: eBPF entry/return uprobes на действительный
  Vulkan dispatch или EGL/GLX swap. Не требует параметров запуска Steam.

## Сборка

Rust stable **1.95+**, Linux x86_64, Vulkan driver, Wayland layer-shell, PipeWire.
Нужны C compiler, pkg-config, clang с BPF target, libclang, development headers
Wayland/xkbcommon/PipeWire/libbpf/libelf/zlib.

openSUSE Tumbleweed:

```sh
sudo zypper install gcc pkg-config clang clang-devel wayland-devel libxkbcommon-devel \
  pipewire-devel libbpf-devel libelf-devel zlib-devel
cargo build --release --workspace
```

Другие дистрибутивы: установить соответствующие `*-dev` / `*-devel` пакеты.
Системный libbpf используется без vendored сборки. `CLANG` может задать путь к compiler.

## Запуск из репозитория

```sh
./target/release/gamebar run
# В другом терминале:
./target/release/gamebar toggle
```

Для немедленного открытия: `./target/release/gamebar run --show`.
Команды управления: `show`, `hide`, `toggle`, `quit`.

### Установка и автозапуск

```sh
bash scripts/install.sh
```

Для SELinux-систем (включая openSUSE) установите privileged helper в системный каталог:

```sh
bash scripts/install.sh --system-helper
```

Этот режим использует sudo только для `/usr/local/libexec/gamebar-fps-helper`,
назначает root ownership и восстанавливает стандартный SELinux context через
`restorecon`. Панель предпочитает этот путь. Запуск privileged helper из `~/.local/bin`
может быть запрещён SELinux (`entrypoint` / `Permission denied`).

Скрипт устанавливает оба бинарника в `~/.local/bin` и desktop entry.
`~/.local/bin` должен быть в PATH.

```sh
mkdir -p ~/.config/autostart
cp ~/.local/share/applications/linux-game-bar.desktop ~/.config/autostart/
```

KDE: запустить `gamebar run`, затем **Win+G**. Если сочетание уже занято, настроить
`Linux Game Bar: toggle overlay` в настройках сочетаний KDE. При обновлении панели
KWin adapter перезагружается. Если KWin был перезапущен, перезапустить панель.

Hyprland (`~/.config/hypr/hyprland.conf`):

```ini
exec-once = gamebar run
bind = SUPER, G, exec, gamebar toggle
```

Sway (`~/.config/sway/config`):

```ini
exec gamebar run
bindsym Mod4+g exec gamebar toggle
```

Глобальное сочетание реализовано compositor bindings; XDG GlobalShortcuts portal
пока не используется. GNOME не является целевой средой.

### FPS

В панели выберите игровой процесс и нажмите **«Включить FPS»**. Откроется системная
авторизация; после неё подключение выполняется автоматически. При следующем Win+G
панель снова откроется (она скрывается для ввода пароля в системном диалоге).
При смене исходного окна
авторизованный helper автоматически подключается к новому выбранному процессу.
Без helper панель явно показывает причину отсутствия FPS.

Панель работает обычным пользователем. Только helper получает privilege:

```sh
gamebar enable-fps
```

Команда запускает системный `gamebar-fps-helper` (или соседний, если системный не установлен)
через `pkexec`. Ошибки авторизации/запуска выводятся в панели и записываются в
`$XDG_RUNTIME_DIR/linux-game-bar/fps-helper.log`. После авторизации
выбрать **точный игровой процесс** → «Подключить FPS». Helper автоматически
отключается после 10 минут без команд панели. «Отключить» снимает probes.
Альтернативный запуск администратором: `sudo /absolute/path/gamebar-fps-helper --uid "$(id -u)"`.

Сокет helper: `/run/linux-game-bar/UID.sock`, доступ только этому UID, плюс
SO_PEERCRED; чужие пользовательские процессы не допускаются. Панель не требует sudo.

**Что измеряется:** cadence успешных present выбранной swapchain/drawable,
среднее frame interval и p95 по kernel monotonic timestamps за 2 секунды.
Выбирается наиболее активная swapchain. Ошибки present не считаются, SUBOPTIMAL
считается успешным Vulkan present. Потери событий инвалидируют полное окно измерения.
Это **не** число кадров, физически показанных дисплеем, и не refresh rate монитора.

Сначала resolver ищет `wsi_QueuePresentKHR` в ELF symbols **самого игрового Mesa ICD**,
включая отдельные custom Mesa builds (например, mesa-overwatch). Если symbols отсутствуют,
Vulkan resolver создаёт собственное временное устройство, получает реальный
`vkGetDeviceProcAddr(vkQueuePresentKHR)`, переводит адрес в on-disk offset и
сопоставляет библиотеку с mappings игры по device/inode. Это обходит простую ошибку tracing loader
export, который приложение может вообще не вызывать. Разные драйверы/версии,
Steam runtime, контейнерные namespaces и Wine могут не совпасть: отображается ошибка.
В игру ничего не инжектируется и её render commands не модифицируются.

**Overwatch/Vulkan:** hot-attach к запущенной игре с custom Mesa успешно получил
present FPS и frame intervals без потерь событий. Это пока одна живая диагностическая
проверка; точность относительно эталонного счётчика и overhead не измерены.
OpenGL и широкая совместимость с играми/anti-cheat пока не проверены.

## Известные границы

- Capture/playback контролируются только при writable SPA Props конкретного потока.
  Нет fallback на silent source для неподдерживаемых capture nodes; такие controls
  disabled. Прямой ALSA и обход PipeWire не охвачены.
- Настройки громкости/mute и тема пока не сохраняются между перезапусками панели.
- Показатели GPU по процессам зависят от DRM fdinfo и прав чтения `/proc`.
  GPU busy — максимум engine utilization для клиента, сумма уникальных клиентов
  приложения; это не универсальная hardware occupancy. CPU приложения может быть
  выше 100% (100% = одно ядро); системный CPU нормирован по всем ядрам.
- RAM приложения — сумма RSS, shared pages могут учитываться повторно.
  System GPU сейчас выбирает доступный sysfs device с большей VRAM, не обязательно
  устройство выбранной игры на multi-GPU машине.
- Группировка cgroup/executable/Wine `.exe` — эвристика; список PID доступен в tooltip.
- Native fullscreen overlay, возврат фокуса, multi-monitor/hotplug, integer HiDPI и
  кириллица реализованы, но runtime acceptance пока не проведён. Fractional scaling,
  clipboard/IME, touch и multi-seat ещё не реализованы полноценно.
- FPS ABI — x86_64 SysV, до 8 swapchains за present; GLX не возвращает success status.
  Probes снимаются при выходе процесса и при штатном завершении панели. При crash панели
  helper снимает их по idle timeout.

## Структура

| Каталог | Назначение |
|---|---|
| `crates/core` | модели snapshots, ProcessId, typed commands, IPC protocol |
| `crates/ui-kit` | Silver Tokens, typography, Painter icons/buttons |
| `crates/gamebar` | layer-shell, egui-wgpu, backend, PipeWire, metrics, compositor adapters |
| `crates/fps-helper` | отдельный privileged service и eBPF programs |
| `integration/kwin` | KWin 6 script: исходное окно + Win+G |

UI не выполняет process scanning или PipeWire operations; snapshots и commands
передаются bounded channels. Hidden UI не рисуется; metrics обновляются раз в секунду.

Лицензирование и происхождение: [LICENSES.md](LICENSES.md).
