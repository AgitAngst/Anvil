# Значок Anvil

Соты: бирюзовое ядро и шесть ячеек вокруг — центр, на котором держится всё остальное.

| Файл | Что это |
| --- | --- |
| `anvil.svg`, `anvil-day.svg` | Основной знак, тёмный и светлый фон (1024×1024). |
| `anvil-micro.svg`, `anvil-micro-day.svg` | Упрощённый знак для 16–32 px: плоское ядро, светлее ячейки, шире зазор. |
| `icon-48.png` … `icon-256.png` | Основной знак, отрисованный на своих размерах. |
| `icon-16.png`, `icon-24.png`, `icon-32.png` | Упрощённый знак на своих размерах (значок в трее — 32). |
| `window-64.png` | Упрощённый знак 64 px для значка окна: Windows сама уменьшает его до 16–32 px. |

`build.rs` собирает из PNG ресурс exe (`icon-16` … `icon-256`), кладёт в `OUT_DIR` сырые RGBA для окна и трея и
`icon-128.png` для уведомлений Windows. Дневные SVG в сборке не участвуют — они для документации.

Перерисовать (нужен [resvg](https://github.com/linebender/resvg), `scoop install resvg`):

```powershell
foreach ($s in 48, 64, 128, 256) { resvg -w $s -h $s anvil.svg icon-$s.png }
foreach ($s in 16, 24, 32) { resvg -w $s -h $s anvil-micro.svg icon-$s.png }
resvg -w 64 -h 64 anvil-micro.svg window-64.png
```
