# Artwork

## Tervia identity

`tervia.svg` is the single-color split T logo, shared by the native welcome screen and Linux package. The macOS packager renders it to PNG and ICNS. `tervia.ico` contains the Windows export at 16, 24, 32, 48, 64, 128, and 256 pixels and is embedded in the Windows executable.

After changing the SVG, regenerate the Windows export on macOS:

```sh
mkdir -p build
swift scripts/icon.swift build/icon.png assets/tervia.ico
```

## Agent glyphs

Claude, Codex (the OpenAI mark), Gemini, Grok, OpenCode and OpenClaw glyphs are sourced from LobeHub's static icon set:

- https://github.com/lobehub/lobe-icons/blob/master/packages/static-svg/icons/claude.svg
- https://github.com/lobehub/lobe-icons/blob/master/packages/static-svg/icons/openai.svg
- https://github.com/lobehub/lobe-icons/tree/master/packages/static-svg/icons

`hermes.svg` is the Hermes Agent master icon from https://github.com/NousResearch/hermes-agent/blob/main/assets/icon-master.svg with its outer background rectangle removed for use as a small glyph. Its source MIT notice is preserved in `hermes-license.txt`.

Both source MIT notices are copied into the app bundle. SVG bytes are embedded in the native executable; they require no network request or WebView at runtime.

## File icons

`material/` contains 55 unmodified SVGs from [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme), pinned to commit `13025adc5ad10a5cc649528a2c1edec93069f1b9`. Original files are under [`icons/` at that revision](https://github.com/material-extensions/vscode-material-icon-theme/tree/13025adc5ad10a5cc649528a2c1edec93069f1b9/icons).

The curated set covers common programming languages, web files, configuration and project files, documents, images and archives. The upstream `toml_light.svg` and `pnpm_light.svg` variants provide contrast in the light theme. Unknown files use the generic document icon. Folder and symlink icons retain the existing native style; ignored file icons use the theme's subdued neutral ink.

File associations and native rendering are shared by the explorer, file tabs (including their drag preview), editor header and filename/text search results in `src/ui/icons.rs`. SVG bytes are embedded, with encoded and decoded images cached for repeated rows. At load time, a 16-by-16 intrinsic viewport bounds raster memory without changing the upstream viewBox or paths. No icon download or extension runtime is needed.

The upstream MIT notice is preserved in `material-license.txt` and included in the macOS, Linux and Windows packages' third-party notices.

## Classic theme palette references

Darcula, Monokai, Solarized Dark/Light and Quiet Light are native Tervia adaptations. Their base colors reference [JetBrains Darcula](https://github.com/JetBrains/intellij-community/blob/master/platform/platform-resources/src/themes/darcula.theme.json), [Darcula editor/console colors](https://github.com/JetBrains/intellij-community/blob/master/platform/platform-resources/src/DefaultColorSchemesManager.xml), and the [VS Code built-in theme palettes](https://github.com/microsoft/vscode/tree/main/extensions). Tervia keeps its own layout and restrained workspace gradients, adjusts UI and syntax inks for readable contrast, and maps token categories to GPUI's highlighter. These presets do not import external extensions or execute theme code.
