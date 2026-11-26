# Hardware reference baselines

Updated: 2026-11-26. This is a **four-tier development target matrix**, not a claim that pokshell currently boots on these handhelds. Tier numbers describe resource and performance classes, not a purchase mandate or a required Steam capability.

| Tier | Representative product | SoC and CPU | GPU | Physical RAM | Display | Primary engineering goal |
| --- | --- | --- | --- | --- | --- | --- |
| 1 — High | Retroid Pocket 6 | Snapdragon 8 Gen 2 (SM8550), 1 prime + 4 performance + 3 efficiency cores | Adreno 740 | 8/12 GB LPDDR5X | 5.5-inch 1920×1080 AMOLED, 120 Hz | Hardware-accelerated compositor, demanding native graphics, frame pacing |
| 1 — High | Retroid Pocket Nova | QCS8550, 1 prime + 4 performance + 3 efficiency cores | Adreno 740 | 8/12 GB LPDDR5X | 4.5-inch 960p AMOLED, 120 Hz | High-performance 4:3 layout and scaling |
| 2 — Upper-mid (A78) | ANBERNIC RG55G1 | Snapdragon G1 Gen 2 (SG4250P; SM4450 Linux platform), 2 Cortex-A78-class + 6 Cortex-A55-class cores | Adreno A12 | 4/8 GB | 5.5-inch 1920×1080 IPS | Modern CPU, modest GPU, scalable graphics quality |
| 2 — Upper-mid (A78) | ANBERNIC RG47G1 (candidate) | Intended G1 Gen 2 class; exact board unverified | TBD | TBD | TBD | Additional G1 Gen 2 form factor; verify model and board before adoption |
| 3 — Mid (A73) | Retroid Pocket Duo Lite | Qualcomm Dragonwing QCS6125, 4 × Cortex-A73 @ 2.0 GHz + 4 × A53 @ 1.8 GHz | Adreno 610 | 4/6 GB LPDDR4X | Top 5.5-inch 1920×1080, 60 Hz; bottom 4.2-inch 960p, 60 Hz; touch | Dual-screen lifecycle, focus, OSK and optional persistent controls |
| 3 — Mid (A73) | MANGMI Air X | Snapdragon 662 (SM6115), 4 × Cortex-A73 + 4 × A53 | Adreno 610 | 4 GB LPDDR4X | 5.5-inch 1920×1080 IPS, 60 Hz; touch | Low-cost single-screen Freedreno baseline |
| 4 — Low | ANBERNIC RG35XX H / RG40XX H (or equivalent) | Allwinner H700, 4 × Cortex-A53, up to 1.5 GHz | Mali-G31 MP2 (manufacturer naming) | 1 GB | 3.5/4-inch 640×480, 4:3 | Minimal memory, efficient rendering, gamepad-first interface |
| 4 — Low | ANBERNIC RG CubeXX | Allwinner H700, 4 × Cortex-A53, up to 1.5 GHz | Mali-G31 MP2 | 1 GB | 3.95-inch 720×720 IPS, 1:1 | Square-screen layout, low-memory composition |
| 4 — Low | ANBERNIC RG DS Plus | RK3568, 4 × Cortex-A55, up to 2.0 GHz | Mali-G52 2EE | 1 GB | Two 4.3-inch 1024×768 IPS panels; lower touch/stylus | Dual-output composition under 1 GB RAM, lid events, touch mapping, Rockchip/Panfrost |

## Additional platform facts

| Feature | Tier 1 — High | Tier 2 — Upper-mid | Tier 3 — Mid | Tier 4 — Low |
| --- | --- | --- | --- | --- |
| Manufacturer system | Android 13 | Android (RG55G1); RG47G1 TBD | Android 15 (Duo Lite), Android 14 (Air X) | Linux |
| Storage | 128 GB UFS 3.1 + microSD | RG55G1 64/128 GB + microSD; RG47G1 TBD | Duo Lite 64/128 GB eMMC; Air X 64 GB eMMC | microSD |
| Cooling | Active | RG55G1 active; RG47G1 TBD | Active | Verify exact board |
| Touch | Single panel | Verify exact board | Duo Lite two panels; Air X single panel | H700 references non-touch; DS Plus lower panel touch/stylus |
| Native Linux development status | Community Linux ports exist | RG55G1 community port exists; RG47G1 unverified | Community Linux ports exist; verify each board and revision | Vendor Linux exists; DS Plus also has a community port; verify each board |
| GPU stack to investigate | Mesa Freedreno/Turnip | Mesa Freedreno/Turnip | Mesa Freedreno/Turnip | Mesa Panfrost; evaluate Vulkan separately |

Community images are starting points, not evidence of complete upstream board support or pokshell acceptance. ROCKNIX release listings include RP6, Nova, RG55G1, Air X and RG DS Plus; Air X documents Fake Suspend rather than verified hardware suspend. Record the actual image, kernel, device tree, Mesa and firmware versions for each test.

### Non-goals and cautions

- A SoC's **nominal Vulkan/OpenGL support is not evidence** that a specific mainline Linux image can run those APIs correctly. Record `glxinfo`, `eglinfo`, `vulkaninfo`, DRM/KMS and real-game evidence per image.
- Per-device Linux capabilities (display, suspend, Wi-Fi, charging, gamepad, touch) remain **unverified by this project**.
- Processor clock, GPU clock and RAM capacity alone do not determine FPS or power efficiency.
- The Low tier is a resource-budget target, not permission to degrade the High-tier experience. In particular, RG DS Plus must fit two outputs into the same 1 GB physical RAM class as H700 devices; do not assume the original RG DS's 3 GB RAM.
- The secondary display is optional on both dual-screen tiers. Games must work on the primary display; verify secondary-output disable/enable and focus independently of game lifetime.
- Native ARM64 games and the native console session remain primary. Steam/FEX/Proton is an optional High-tier workload, not a prerequisite for device selection or release acceptance.
- The original RG DS is not a primary procurement target; use RG DS Plus as the low-memory dual-screen reference. Do not treat firmware or device trees as interchangeable.
- RK3576 / RG Vita Pro remains out of the primary reference set because its currently relevant Linux graphics stack is less compelling for the project. This is a project prioritization, not a claim of hardware incompatibility.
- Product availability and price fluctuate; they are intentionally excluded from this long-lived specification.

## Power and thermal acceptance

Treat power/thermal validation as a blocking gate before recommending a device-specific image for routine use. Industrial design, brand reputation and a successful boot do not establish battery safety.

- Record exact board revision, battery label/specification, supported charging modes, charger/PMIC and device-tree configuration. Verify charging limits against the actual battery specification; do not guess values or reuse another board's configuration.
- Verify credible battery voltage/current/temperature readings, thermal zones, thermal protection, charge termination, and normal behavior during supervised load, idle, lid-close, suspend/resume and shutdown. Record missing telemetry and unverified protections as blockers.
- Do not disable thermal or charging protection, deliberately overheat or overcharge a device, or perform unattended/overnight charging with unvalidated firmware. Stop testing if swelling, odor or abnormal heating occurs.
- Record known board-specific quality issues, hardware revisions, recovery instructions and battery replacement/serviceability. Keep reported incidents distinct from verified defects and do not infer population failure rates from anecdotes.
- Maintain the existing non-root seat and physical-device evidence requirements; no firmware flashing without device-specific instructions and user approval.

## Manufacturer and community references

1. ANBERNIC RG35XX H: https://anbernic.com/products/rg35xx-h
2. ANBERNIC RG40XX H: https://anbernic.com/products/rg40xx-h
3. Retroid Pocket Duo Lite: https://www.goretroid.com/products/retroid-pocket-duo-lite-handheld
4. Retroid Pocket 6: https://www.goretroid.com/products/retroid-pocket-6-handheld
5. Retroid Pocket Nova: https://www.goretroid.com/collections/frontpage/products/retroid-pocket-nova-handheld
6. ANBERNIC RG55G1: https://anbernic.com/products/rg-55g1
7. MANGMI Air X: https://mangmi.com/collections/all/products/mangmi-air-x
8. ANBERNIC RG CubeXX: https://anbernic.com/products/rg-cubexx
9. ANBERNIC RG DS Plus: https://anbernic.com/products/rg-ds-plus
10. ANBERNIC RG DS / DS Plus comparison: https://anbernic.com/a/shop/content/articles/news/rg-ds-plus-vs-rg-ds
11. ROCKNIX Air X documentation: https://rocknix.org/devices/mangmi/air-x/
12. ROCKNIX release matrix: https://releases.rocknix.org/

The Retroid Duo Lite product page lists **Duo Lite and Duo Lite Plus separately**. This project targets QCS6125 Duo Lite, *not* Q-7790 Duo Lite Plus. RG47G1 is an explicitly provisional candidate pending a reliable manufacturer specification and device-level Linux evidence.
