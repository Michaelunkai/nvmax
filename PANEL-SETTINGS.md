# NVIDIA Control Panel > Manage 3D settings > Global Settings (measured 2026-11-18, fresh panel, after nvmax)

| Setting | Value shown | Max-performance? |
|---|---|---|
| Image Scaling | Off | yes |
| Ambient Occlusion | Off | yes |
| Anisotropic filtering | 16x | quality option, set by user demand "every setting" |
| Antialiasing - FXAA | Off | yes |
| Antialiasing - Gamma correction | On | default, no perf impact (not settable via driver ID found) |
| Antialiasing - Mode / Setting | Application-controlled | yes |
| Antialiasing - Transparency | Off | yes |
| Background Application Max Frame Rate | Off | yes |
| CUDA - GPUs | All | yes |
| CUDA - Sysmem Fallback Policy | Driver Default | open: see CHECKLIST |
| DSR - Factors / Smoothness | Off | yes |
| Low Latency Mode | Ultra (via panel UI Apply, no driver ID) | yes |
| Max Frame Rate | Off | yes |
| Monitor Technology | Fixed Refresh (driver 0x10A879CF=4) | yes |
| Multi-Frame Sampled AA (MFAA) | Off | yes |
| OpenGL GDI compatibility | Auto | default |
| OpenGL rendering GPU | Auto-select | default |
| Power management mode | Prefer maximum performance | yes |
| Preferred refresh rate | Highest available | yes |
| Shader Cache Size | Unlimited | yes |
| Texture filtering - Anisotropic sample optimization | On | yes |
| Texture filtering - Negative LOD bias | Clamp | default |
| Texture filtering - Quality | High performance | yes |
| Texture filtering - Trilinear optimization | Off | shown by panel; driver value 1 |
| Threaded optimization | On | yes |
| Triple buffering | Off | yes |
| Vertical sync | Off | yes |
| Virtual Reality pre-rendered frames | 1 | yes |
| Virtual Reality - Variable Rate Super Sampling | Off | yes |
| Vulkan/OpenGL present method | Auto | default |

## Walk 2026-11-18 (screenshots final-global.png, walk-global-scrolled.png, walk-program2.png)
Global: all rows max (Ultra LL, Fixed Refresh, 16x aniso, Prefer max perf, Shader cache Unlimited, Quality High performance, Trilinear On (driver value 0), AO/FXAA/VSync/FrameCap Off, Threaded On, prerender 1).
Program Settings: every row "Use global setting" (inherits global). Sysmem Fallback: driver rejects store (id not found) - left Driver Default.
