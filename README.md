# Linux Game Bar

Нативная игровая панель для **KDE Plasma Wayland, Hyprland и Sway**:
Rust, Smithay Client Toolkit, egui 0.36 и wgpu 30. Дизайн адаптирован из
`rust-gui-silver-bullet`: Pro Dark / Studio Light / Modern Dark, Painter-иконки,
локальные шрифты с кириллицей.

**Текущий релиз: v0.1.6.**
Сборка проходит на openSUSE Tumbleweed; FPS hot-attach работает в живой сессии Overwatch.
Автоматизированные тесты по просьбе автора не запускаются. [Точный статус](verification/RESULTS.md).

## Возможности

- Полноэкранная прозрачная Wayland **overlay layer**. При возврате в игру остаются
  только метрики: полупрозрачный **click-through HUD**, без клавиатурного фокуса.
  HUD можно выключить; тогда surface и GPU presentation уничтожаются.
- **Win+G в KDE** через автоматически загружаемый KWin 6 script. Esc возвращает в игру.
  В Hyprland/Sway используется compositor key binding на `gamebar toggle`.
- Панель перетаскивается за заголовок, меняет размер за края; slider «Масштаб»
  меняет размер интерфейса. Каждая метрика — отдельный перемещаемый и изменяемый
  по размеру виджет; × скрывает его, меню «Виджеты и HUD…» возвращает.
  Позиции, размеры, видимость, тема, масштаб и прозрачность сохраняются в
  `$XDG_CONFIG_HOME/linux-game-bar/layout.json` (по умолчанию `~/.config/...`).
- Карандаш в каждом виджете открывает его настройки: отдельная непрозрачность фона
  **в игре** 0–100% и интервал обновления 250 / 500 ms / 1 / 2 / 5 / 10 s.
  При удержании ползунка меню и его затемнение скрываются: виден реальный HUD
  поверх игры и маленькое окно настройки. В меню виджеты остаются непрозрачными.
  «Использовать общий фон HUD» возвращает наследование.
  Интервал управляет обновлением показания, не статистическим окном AVG/lows.
- Виджеты прилипают к краям экрана и соседям; после перемещения/изменения размера
  выбирается ближайшая свободная позиция без перекрытия, если на экране хватает места.
- Сетка в меню и привязка метрик: шаг 8/16/24/32/48/64 логических пикселя.
  «Виджеты и HUD…» позволяет отдельно включать сетку/привязку и выровнять все метрики.
  Перетаскивание за заголовок управляется панелью: позиция рассчитывается по мыши
  и округляется к ячейкам **до отрисовки**, а не корректируется после свободного drag.
  Окна меню, настроек и HUD имеют прямые углы, без закруглений.
  Grid-позиция остаётся точной, а визуальное движение между ячейками плавное.
  При наезде перемещаемое окно выталкивает перекрываемые метрики, включая цепочки
  соседей. Если направление упирается в край экрана, пробуются другие стороны.
  Только когда места нет, сохраняется последняя допустимая раскладка; основное
  окно настроек/ресурсов остаётся неподвижным препятствием.
  Ширина и высота метрик также кратны шагу сетки (включая заголовок и рамку).
  Изменение размера — за ручку в правом нижнем углу; при увеличении соседние окна
  выталкиваются. Сохранённые размеры округляются вверх без сброса раскладки.
  Отключение привязки возвращает изменение размеров с шагом в один пиксель.
  Сетка не показывается в игровом HUD и предпросмотре прозрачности.
- Метрики сохраняются на обеих вкладках, включая «Ресурсы». Масштаб меняется
  непрерывно, с плавным переходом к выбранному значению и кадрами анимации до 60 Hz;
  после завершения анимации дополнительный repaint прекращается.
- Переключатель «Оставлять HUD поверх игры» и отдельное назначаемое сочетание для
  скрытия/показа метрик без меню (по умолчанию **Meta+Shift+G**).
  Задаётся в «Виджеты и HUD…» → «Применить сочетание». Поддерживаются Meta/Super,
  Ctrl, Shift, Alt и A–Z/0–9/F1–F12. Команда для ручной привязки: `gamebar hud-toggle`.
- Собственный Wayland cursor устанавливается сразу при pointer enter; resize/drag
  cursors синхронизируются с egui, не наследуя скрытый курсор игры.
- Автоматический выбор процесса исходного активного окна и монитора перед захватом
  фокуса панелью; отдельного process picker нет. HUD хранит цель последнего открытия.
- Native PipeWire-мониторинг потоков: громкость, отдельное отключение **звука приложения**
  и **его входящего потока микрофона**, без глобального выключения устройства.
  Политики действуют на новые потоки приложения в течение текущего запуска панели.
  PID/имя берутся из node props или связанного PipeWire Client через `client.id`;
  это важно для Wine/Proton и Pulse-compatible потоков. Служебные monitor/capture-sink
  потоки измерителей громкости исключены. `gamebar audio-list` выводит реальный
  snapshot потоков и доступность controls для диагностики.
- Две вкладки: «Производительность» и «Ресурсы». Громкость и mute звука/микрофона
  находятся прямо у CPU/GPU-интенсивных приложений и в таблице ресурсов.
  Активные аудиоприложения также включаются в список, даже при небольшой нагрузке.
- `/proc` CPU, RSS RAM, I/O; DRM fdinfo GPU/VRAM по приложению; sysfs/hwmon
  системные GPU, VRAM и температуры. Недоступные данные обозначены `—`.
- CPU приложения и системы — шкала **0–100% относительно всех online логических CPU**.
  GPU приложения — максимум среди движков устройства после суммирования его уникальных
  DRM-клиентов, с ограничением 100%. Это оценка fdinfo, не универсальная GPU occupancy.
- Отдельные окна «Диск приложения» и «Диски системы»: чтение и запись в B/s,
  KiB/s, MiB/s. Приложение — `/proc/PID/io` read_bytes/write_bytes; система —
  физические block devices из `/proc/diskstats`, без повторного подсчёта разделов,
  loop и device-mapper. Работа через RAM/page cache может не давать чтения с диска;
  буферизованная запись приложения и физический writeback могут происходить в разное время.
  Виджеты автоматически добавляются в существующую раскладку, их можно переместить/скрыть.
  Диагностика: `gamebar metrics-status [PID]`.
- Список приложений, сортировка CPU/RAM/GPU/I/O, поиск, закрытие исходного окна,
  SIGTERM/SIGKILL через pidfd с проверкой starttime.
- Экспериментальный FPS hot-attach helper: eBPF entry/return uprobes на действительный
  Vulkan dispatch или EGL/GLX swap. Не требует параметров запуска Steam.
- Текущий present FPS, AVG, 1% low, 0,1% low и график отдельных present-интервалов.
  У графика min/max envelope: короткие пики не исчезают при сжатии по ширине.
  Шкала — последние 10 секунд по CLOCK_MONOTONIC, непрерывная прокрутка до 30 Hz;
  новые samples добавляются по timestamp без переукладки всего графика. Плавная
  смена Y scale не сглаживает измеренные пики.

## Готовый релиз

Скачайте архив `linux-game-bar-0.1.6-linux-x86_64.tar.gz` со страницы
[Releases](https://github.com/DurkaEbanaya/linux-game-bar/releases), распакуйте и запустите:

```sh
bash scripts/install.sh --system-helper
~/.local/bin/gamebar run
```

Бинарники собраны на openSUSE Tumbleweed x86_64 с системными shared libraries;
нужны совместимые glibc, libxkbcommon, PipeWire, libbpf 1.x, libelf/zlib и Vulkan/Wayland.
Для более старых дистрибутивов используйте сборку из исходников ниже.
Рядом опубликованы SHA-256 и полный исходный архив соответствующего релиза.
После обновления остановите старый FPS-helper и заново включите FPS: замена
бинарника на диске не обновляет уже работающий процесс. Для helper в transient
service: `sudo systemctl stop linux-game-bar-fps`; для запущенного через pkexec —
`sudo pkill -x gamebar-fps-hel` (имя процесса Linux ограничено 15 символами).
Затем «Включить FPS» в панели. Старый helper без timestamps теперь явно отмечается
в виджете Frametime вместо молча пустого графика.

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

Откройте Win+G из игры и нажмите **«Включить FPS»**. Откроется системная
авторизация; после неё подключение выполняется автоматически. При следующем Win+G
панель снова откроется (она скрывается для ввода пароля в системном диалоге).
При смене исходного окна
авторизованный helper автоматически подключается к процессу исходного окна.
Без helper панель явно показывает причину отсутствия FPS.

Панель работает обычным пользователем. Только helper получает privilege:

```sh
gamebar enable-fps
```

Команда запускает системный `gamebar-fps-helper` (или соседний, если системный не установлен)
через `pkexec`. Ошибки авторизации/запуска выводятся в панели и записываются в
`$XDG_RUNTIME_DIR/linux-game-bar/fps-helper.log`. После авторизации
открыть панель из игры → «Переподключить FPS» при необходимости. Helper автоматически
отключается после 10 минут без команд панели. «Отключить» снимает probes.
Альтернативный запуск администратором: `sudo /absolute/path/gamebar-fps-helper --uid "$(id -u)"`.

Сокет helper: `/run/linux-game-bar/UID.sock`, доступ только этому UID, плюс
SO_PEERCRED; чужие пользовательские процессы не допускаются. Панель не требует sudo.

**Что измеряется:** cadence успешных present выбранной swapchain/drawable,
среднее frame interval и p95 по kernel monotonic timestamps за 2 секунды.
Helper передаёт интервалы за 2 s (до 4096) с timestamp; UI объединяет перекрывающиеся
snapshots в 10-секундную историю графика, до 20000 samples.
AVG — число интервалов / их суммарная длительность за последние **до 30 секунд**.
1% low и 0,1% low — `1000 / среднее время самых медленных ceil(N × 0.01/0.001)
интервалов в миллисекундах`. Значения доступны начиная со 100 / 1000 интервалов;
до этого `—`. Длительность и размер статистической выборки указаны в панели.
Attach, смена swapchain и потеря событий сбрасывают статистику.
Выбирается наиболее активная swapchain. Ошибки present не считаются, SUBOPTIMAL
считается успешным Vulkan present. Потери событий инвалидируют полное окно измерения.
Это **не** число кадров, физически показанных дисплеем, и не refresh rate монитора.
Frametime здесь означает расстояние между успешными present, а не длительность
GPU rendering. Статистика включает время открытого меню: автоматической паузы нет.

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
- Настройки громкости/mute пока не сохраняются между перезапусками панели;
  раскладка виджетов и настройки HUD/темы сохраняются.
- Показатели GPU по процессам зависят от DRM fdinfo и прав чтения `/proc`.
  GPU busy — сумма уникальных клиентов по одному device/engine, затем максимум
  среди движков с ограничением 100%; это не универсальная hardware occupancy.
  CPU приложения нормирован по всем online логическим ядрам, как системный CPU.
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
передаются bounded channels. FPS backend обновляется каждые 250 ms; CPU/GPU/RAM —
раз в секунду. Показания виджетов обновляются по их индивидуальным таймерам;
без HUD скрытый UI не рисуется. Полное завершение: `gamebar quit`.

Лицензирование и происхождение: [LICENSES.md](LICENSES.md).
