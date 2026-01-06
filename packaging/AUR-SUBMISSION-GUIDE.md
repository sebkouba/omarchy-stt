# AUR Submission Guide for omarchy-stt-bin

## Prerequisites

1. **AUR Account**: Register at https://aur.archlinux.org/register/
2. **SSH Key**: Add your SSH public key to your AUR account settings
3. **Tools**: Install `base-devel` and `git`

```bash
sudo pacman -S base-devel git
```

## Step-by-Step Release Process

### 1. Create a GitHub Release

```bash
# Build and create release tarball (updates version from Cargo.toml)
./scripts/create-release.sh

# Or specify version manually
./scripts/create-release.sh 0.1.0
```

This creates `releases/omarchy-stt-0.1.0-x86_64.tar.gz` and prints the SHA256 checksum.

### 2. Publish GitHub Release

1. Go to https://github.com/yourusername/transcribe-rs-v2/releases/new
2. Create tag: `v0.1.0`
3. Release title: `v0.1.0 - Initial Release`
4. Upload the tarball: `releases/omarchy-stt-0.1.0-x86_64.tar.gz`
5. Publish release

### 3. Update PKGBUILD

Edit `packaging/PKGBUILD`:
- Update `pkgver=0.1.0` to your version
- Update `url=` to your actual GitHub repo
- Update the first `sha256sum` with the checksum from step 1
- Update `# Maintainer:` line with your info

```bash
cd packaging
# Generate .SRCINFO (required for AUR)
makepkg --printsrcinfo > .SRCINFO
```

### 4. Test the Package Locally

```bash
cd packaging
makepkg -si
# This downloads the release, builds the package, and installs it
```

Test that everything works:
```bash
systemctl --user start recording-daemon transcribe-daemon
transcribe --version
```

### 5. Submit to AUR (First Time)

```bash
# Clone your AUR repository
cd ~/aur  # or wherever you keep AUR packages
git clone ssh://aur@aur.archlinux.org/omarchy-stt-bin.git
cd omarchy-stt-bin

# Copy package files
cp /path/to/transcribe-rs-v2/packaging/PKGBUILD .
cp /path/to/transcribe-rs-v2/packaging/.SRCINFO .
cp /path/to/transcribe-rs-v2/packaging/omarchy-stt-bin.install .

# Commit and push
git add PKGBUILD .SRCINFO omarchy-stt-bin.install
git commit -m "Initial commit: omarchy-stt-bin 0.1.0"
git push
```

Your package is now live at: https://aur.archlinux.org/packages/omarchy-stt-bin

### 6. Update Package (For New Releases)

```bash
# 1. Create new GitHub release (repeat step 1-2)
./scripts/create-release.sh 0.2.0

# 2. Update PKGBUILD
cd ~/aur/omarchy-stt-bin
# Edit PKGBUILD: bump pkgver, update sha256sum, reset pkgrel=1
vim PKGBUILD

# 3. Regenerate .SRCINFO
makepkg --printsrcinfo > .SRCINFO

# 4. Test locally
makepkg -si

# 5. Push to AUR
git add PKGBUILD .SRCINFO
git commit -m "Update to version 0.2.0"
git push
```

## Maintenance Tips

### Getting the Parakeet Model Checksum

If you need to verify the Parakeet model checksum:

```bash
wget https://blob.handy.computer/parakeet-v3-int8.tar.gz
sha256sum parakeet-v3-int8.tar.gz
# Add this to PKGBUILD sha256sums array
```

### Common Issues

**"Unknown public key" error during makepkg:**
- Import the GPG key: `gpg --recv-keys KEYID`

**Build fails with "source not found":**
- Make sure the GitHub release is published and public
- Verify the URL in PKGBUILD matches your repo

**Systemd services don't start:**
- Check user has enabled services: `systemctl --user enable recording-daemon`
- Verify paths in service files are correct

### AUR Package Guidelines

Follow these conventions:
- Package name should be lowercase with hyphens: `omarchy-stt-bin`
- `-bin` suffix indicates precompiled binaries
- `pkgrel` starts at 1, increment for PKGBUILD-only changes
- Reset `pkgrel=1` when `pkgver` changes
- Keep `.SRCINFO` in sync (regenerate after every PKGBUILD change)
- Test before pushing!

### Resources

- AUR Submission Guidelines: https://wiki.archlinux.org/title/AUR_submission_guidelines
- PKGBUILD Reference: https://wiki.archlinux.org/title/PKGBUILD
- AUR Web Interface: https://aur.archlinux.org
