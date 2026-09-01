# ❄️ Flaker

An interactive TUI manager for administering and maintaining **NixOS** configurations based on **Flakes**, written in **Rust** using [**Ratatui**](https://github.com/ratatui/ratatui).

Flaker requires a [Nerd Font](https://www.nerdfonts.com/) (or a Nerd Font patched terminal font) to render its icons correctly.

---

## 📦 Declarative Installation (Flakes)

### 1. Add to your system `flake.nix`

Add Flaker to your `inputs`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    
    # Flaker input
    flaker.url = "github:tw1xale/flaker";
  };

  outputs = { self, nixpkgs, flaker, ... }@inputs: {
    nixosConfigurations.home = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      specialArgs = { inherit inputs; };
      modules = [
        ./configuration.nix
      ];
    };
  };
}
```

### 2. Add package to `configuration.nix` (or `home.nix`)

```nix
{ pkgs, inputs, ... }:

{
  environment.systemPackages = [
    inputs.flaker.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

*(Alternatively, you can use the overlay: `nixpkgs.overlays = [ inputs.flaker.overlays.default ];` and install `pkgs.flaker`).*

After rebuilding your system (`nixos-rebuild switch`), the `flaker` command will be globally available in your terminal:

```bash
flaker
```

---

## ⚡ Quick Run (Without Installing)

You can run Flaker instantly without installing:

```bash
nix run github:tw1xale/flaker
```

---

## ✨ Features & Menu Structure

The application organizes all system management actions into logical submenus:

### 🔨 Updates
- **Rebuild System (`rebuild switch`)**: Automatically checks for unstaged/staged changes, prompts for a commit message (with sensible default), commits, builds, switches configuration, and pushes to remote.
- **Update Lockfile (`flake update`)**: Updates `flake.lock`, commits only the lockfile, and pushes to remote.
- **Full Cycle (`update + switch`)**: Runs `flake update`, commits pending changes, rebuilds & activates the new generation, and pushes to remote.
- **Test Build**: Executes `nixos-rebuild build` to verify configuration compilation without activating it on the running system.

### 🧹 Maintenance
- **Clean Garbage & Optimize Store**: Runs a three-step cleanup (`nix-collect-garbage -d` → `nix-store --gc` → `nix-store --optimise`). *Requires explicit confirmation.*
- **System Generations History**: Inspects all system generations via an interactive, scrollable pager.

### 📜 Git & History
- **Show Working Changes (Git Diff)**: Integrated scrollable diff viewer with syntax highlighting (green for additions, red for deletions, mauve for hunk headers). Shows a clean status when no changes are present.
- **Hard Reset Rollback (`git reset --hard`)**: Interactive fuzzy commit selector → explicit warning screen → reset → force push → switch system. *Requires explicit confirmation.*
- **Soft Revert Rollback (`git checkout <hash> -- .`)**: Interactive commit selector → restores tree state → records a new commit → pushes and switches system.
- **Trim History (`git reset --soft`)**: Interactive commit selector → squashes commit history back to target commit while keeping working tree files untouched → force push. *Requires explicit confirmation.*

---

## 🛡️ Safety First

Destructive operations (such as Hard Reset, Store Cleanup, and Force Pushes) **always require explicit confirmation** through dedicated modal dialogs with clear warnings.

---

## 🎨 Themes & Styling

Flaker includes built-in palettes (**Catppuccin** Mocha/Macchiato/Frappé/Latte, **Nord**, **Tokyo Night**, **Dracula**, **Gruvbox**) and supports custom HEX overrides in `config.toml`:

- **Terminal Transparent**: Never forces an opaque background fill, perfectly matching your terminal emulator's opacity and blur settings.

---

## 🛠️ Local Development & Build

### Development Shell
```bash
nix develop
```

### Local Cargo Build
```bash
# Debug build
cargo build

# Release build (binary located at ./target/release/flaker)
cargo build --release
```

---

## ⚙️ Configuration & Universal Auto-Detection

Flaker automatically detects your NixOS setup, permissions, and Git integration without requiring manual configuration:

- **Smart Directory Discovery**: Searches current directory (`.`), `~/.config/nixos`, `~/dotfiles/nixos`, `~/dotfiles`, `~/nixos-config`, `~/nixos`, and `/etc/nixos`.
- **Target Auto-Detection**: Inspects `flake.nix` and matches `nixosConfigurations.<name>` against your system hostname or username.
- **Root & User-Space Support**: If your configuration resides in your home directory, Git and Flake operations run without `sudo` (preserving proper file ownership and permissions). `sudo` is only invoked when elevated privileges are actually required (e.g. `nixos-rebuild switch` or `/etc/nixos`).
- **Standalone / Non-Git Support**: If your Flake is not a Git repository, system builds and flake updates work seamlessly out-of-the-box without Git prompts or errors.

To explicitly override the target or directory, use the environment variables:

```bash
# Explicit target override
FLAKER_TARGET="/home/username/dotfiles#my-host" flaker

# Or explicit directory override
FLAKER_DIR="/home/username/dotfiles" flaker
```

---

## ⌨️ Custom Configuration (`config.toml`)

Flaker automatically creates a configuration file at `~/.config/flaker/config.toml` (or `~/.config/flaker.toml`) on first launch with sensible defaults.

You can customize custom flake paths, color palettes, auto-commit message templates, and keybindings:

```toml
# ~/.config/flaker/config.toml

[general]
# Explicit flake directory path (leave empty for automatic detection)
# Examples: "/etc/nixos", "~/dotfiles/nixos", "~/my-config"
flake_dir = ""

# Explicit flake target (leave empty for automatic detection)
# Examples: "/etc/nixos#hostname", "~/dotfiles#my-user"
flake_target = ""

[theme]
# Color palette. Available options:
# "mocha" (default dark), "macchiato", "frappe", "latte" (light),
# "nord", "tokyonight", "dracula", "gruvbox"
palette = "mocha"

# Optional custom color overrides (HEX format like "#89b4fa"):
# border = "#89b4fa"
# accent = "#cba6f7"
# selected = "#f9e2af"
# header_title = "#89dceb"

[commit_templates]
# Default commit messages for actions
rebuild = "rebuild"
flake_update = "flake update"
full_cycle = "full update"
soft_revert = "revert to {hash}"
trim_history = "trim history to {hash}"

[keybindings]
# Enable instant single-digit selection (1, 2, 3...) for menu items
enable_quick_digits = true

# Key assigned to the Back / Exit menu item (e.g. "q", "b", "Esc")
back_item_key = "q"

# Navigation keys
up = ["Up", "k"]
down = ["Down", "j"]
page_up = ["PageUp"]
page_down = ["PageDown"]
home = ["Home"]
end = ["End"]

# Action keys
select = ["Enter"]
back = ["Esc"]
quit = ["q", "Ctrl-c"]
clear_input = ["Ctrl-u"]
```

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
