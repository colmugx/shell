# Hardware reference baselines

Updated: 2026-11-20. This is a **development target matrix**, not a claim that pokshell currently boots on these handhelds.

| Tier | Representative product | SoC and CPU | GPU | Physical RAM | Display | Primary engineering goal |
| --- | --- | --- | --- | --- | --- | --- |
| Low | ANBERNIC RG35XX H / RG40XX H (or equivalent) | Allwinner H700, 4 × Cortex-A53, up to 1.5 GHz | Mali-G31 MP2 (manufacturer naming) | 1 GB | 3.5 / 4 inch, 640×480, 4:3 | Minimal memory, efficient rendering, gamepad-first interface |
| Mid | Retroid Pocket Duo Lite | Qualcomm Dragonwing QCS6125, 4 × Cortex-A73 @ 2.0 GHz + 4 × A53 @ 1.8 GHz | Adreno 610 @ 950 MHz | 4 GB or 6 GB LPDDR4X | Top 5.5 inch 1920×1080, 60 Hz; bottom 4.2 inch 960p, 60 Hz; touch | Dual-screen lifecycle, focus, OSK and optional persistent controls |
| High | Retroid Pocket 6 | Snapdragon 8 Gen 2; vendor lists 1 GoldPlus @ 3.2 GHz + 4 Gold @ 2.8 GHz + 3 Silver @ 2.0 GHz | Adreno 740 @ 680 MHz | 8 GB / 12 GB LPDDR5X | 5.5 inch 1920×1080 AMOLED 120 Hz | Hardware accelerated compositor, stable frame pacing, demanding native graphics |

## Additional platform facts

| Feature | Low | Mid | High |
| --- | --- | --- | --- |
| Default system from manufacturer | Linux | Android 15 | Android 13 |
| Storage | microSD | 64/128 GB eMMC | 128 GB UFS 3.1 |
| Cooling | Passive | Active | Active |
| Touchscreen | No (reference models) | Two touch panels | Touchscreen |
| Native Linux development status | Linux system exists; board-specific drivers still required | Community Linux ports exist; must validate exact image and hardware functions | Community Linux ports exist; must validate exact image and hardware functions |
| GPU stack to investigate | Mesa Panfrost | Mesa Freedreno/Turnip | Mesa Freedreno/Turnip |

### Non-goals and cautions

- A SoC's **nominal Vulkan/OpenGL support is not evidence** that a specific mainline Linux image can run those APIs correctly. Record `glxinfo`, `eglinfo`, `vulkaninfo`, DRM/KMS and real-game evidence per image.
- Per-device Linux capabilities (display, suspend, Wi-Fi, charging, gamepad, touch) remain **unverified by this project**.
- Processor clock, GPU clock and RAM capacity alone do not determine FPS or power efficiency.
- The Low tier is a resource-budget target, not permission to degrade the High-tier experience.
- The Mid tier's lower screen is optional. Games must work on the primary display.
- RK3576 / RG Vita Pro is deliberately **out of the primary reference set** because its currently relevant Linux graphics stack is less compelling for the project. This is a project prioritization, not a claim of hardware incompatibility.
- Product availability and price fluctuate; they are intentionally excluded from this long-lived specification.

## Manufacturer specifications

1. ANBERNIC RG35XX H: https://anbernic.com/products/rg35xx-h
2. ANBERNIC RG40XX H: https://anbernic.com/products/rg40xx-h
3. Retroid Pocket Duo Lite: https://www.goretroid.com/products/retroid-pocket-duo-lite-handheld
4. Retroid Pocket 6: https://www.goretroid.com/products/retroid-pocket-6-handheld

The Retroid Duo Lite product page lists **Duo Lite and Duo Lite Plus separately**. This project targets QCS6125 Duo Lite, *not* Q-7790 Duo Lite Plus.
