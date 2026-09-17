# mayhem_tcp branding

`mayhem_crab.png` is the approved red-gradient circuit crab logo, generated
with OpenAI image generation and selected by the project owner on 2026-09-17.
The 1254 × 1254 PNG has a transparent background. It is included under the
repository's MIT license. This is mayhem_tcp project artwork, not a Realtek
logo or an indication of affiliation.

The repository README and Android launcher use this same file. The Android
project links it as a drawable resource; no separate resized copy or image
generation step is required at build time. The adaptive icon places the logo
over white, with a 20% foreground inset to protect the claws and legs from
launcher masks. Android applies scaling and filtering at runtime.

Adaptive icons are supported throughout the app's Android API 28+ target range.
See [Android adaptive icon guidance](https://developer.android.com/develop/ui/compose/system/icon_design_adaptive).

Verified on 2026-09-17: ARM64 and x64 APK builds passed with no warnings or
errors; packaged manifest references the adaptive icon. Installed the x64 APK
in the Android 16 emulator and visually checked the circular launcher icon:
red gradient, complete claws/legs and readable silhouette at launcher size.
The ARM64 APK still needs a physical-phone launcher check.
