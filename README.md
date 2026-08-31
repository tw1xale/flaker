# ❄️ Flaker

An interactive TUI manager for administering and maintaining **NixOS** configurations based on **Flakes**, written in **Rust** using [**Ratatui**](https://github.com/ratatui/ratatui) and the [**Catppuccin Mocha**](https://github.com/catppuccin/catppuccin) color palette.

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
    inputs.flaker.packages.${pkgs.system}.default
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
- **Test Build (`dry run`)**: Executes `nixos-rebuild build` to verify configuration compilation without touching the running system.

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

## 🎨 Theme

Styled with the [**Catppuccin Mocha**](https://github.com/catppuccin/catppuccin) dark palette:
- **Blue**: Borders & active cursor indicators
- **Sky**: Headers and titles
- **Sapphire**: Flake targets and metadata
- **Peach / Red**: Warning & danger dialogs
- **Green**: Successful operations & diff additions
- **Yellow**: Active menu selection highlight
- **Mauve**: Section titles & match highlights
- **Terminal Transparent**: Never forces a background fill, perfectly matching your terminal emulator's opacity settings.

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

## 📄 License

This project is licensed under the [MIT License](LICENSE).
