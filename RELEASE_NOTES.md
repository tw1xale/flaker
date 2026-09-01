# ❄️ Flaker v0.2.0

**Flaker v0.2.0** brings major enhancements, including custom configuration files, rich color palettes, instant digit navigation, flexible keybindings, custom commit message templates, and extensive error-handling improvements!

---

### ✨ What's New in v0.2.0

#### 🎨 Color Palettes & Theming
- **8 Built-in Palettes**: Catppuccin (*Mocha, Macchiato, Frappé, Latte*), Nord, Tokyo Night, Dracula, and Gruvbox.
- **Custom HEX Overrides**: Set custom HEX colors in `config.toml` (`border`, `accent`, `selected`, `header_title`, etc.).
- **Dynamic CLI Banners**: Command execution banners now dynamically adapt to your active theme palette.
- **Full Terminal Transparency**: Works seamlessly with transparent and blurred terminal backgrounds.

#### ⌨️ Ergonomic Navigation & Configurable Keybindings
- **Single-Digit Selection**: Jump directly to menu actions using number keys (`1`, `2`, `3`...).
- **Dedicated Back / Exit Key**: The last menu item is assigned to a fixed shortcut (defaults to `q. Exit` / `q. Back`), preserving muscle memory across all submenus.
- **Full Key Customization**: Configure navigation, selection, modal actions, and cancel keys in `~/.config/flaker/config.toml`.
- **Adaptive Footer Hints**: Hints are rendered dynamically across the terminal width to prevent text truncation.

#### 📝 Commit Templates & Path Overrides
- **Custom Commit Templates**: Customize default commit messages for rebuilds, lockfile updates, and rollbacks.
- **Custom Paths & Targets**: Explicitly configure `flake_dir` and `flake_target` in `[general]` or via environment variables (`FLAKER_DIR`, `FLAKER_TARGET`).

#### 🛡️ Stability & Polish
- Robust error checking on Git staging and commit operations.
- Accurate error attribution separating network push issues from Nix build failures.
- Uninhibited text filtering for Git commit history search.
- Clean rounded UI boxes with separated status hints.

---

### 📦 Installation

#### Declarative Installation via Flakes:
```nix
# flake.nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flaker.url = "github:tw1xale/flaker";
  };
  # ...
}

# configuration.nix
{ pkgs, inputs, ... }: {
  environment.systemPackages = [
    inputs.flaker.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

#### Quick Run:
```bash
nix run github:tw1xale/flaker
```
