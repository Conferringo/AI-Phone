#!/bin/sh
set -eu
home=${HOME:?Desktop home missing}
case "${1:-}" in
prepare)
  mkdir -p "$home/Files" "$home/Desktop" "$home/.config/openbox" "$home/.config/pcmanfm/default" "$home/.config/libfm"
  if [ ! -f "$home/.config/libfm/libfm.conf" ]; then
    cat > "$home/.config/libfm/libfm.conf" <<'FILE_MANAGER'
[config]
quick_exec=1
FILE_MANAGER
  fi
  if [ ! -f "$home/.config/pcmanfm/.ai-phone-style-v1" ]; then
    cat > "$home/.config/pcmanfm/default/desktop-items-0.conf" <<'DESKTOP_STYLE'
[*]
wallpaper_mode=color
desktop_bg=#e7ebef
desktop_fg=#26313c
desktop_shadow=#ffffff
show_documents=0
show_trash=0
show_mounts=0
DESKTOP_STYLE
    touch "$home/.config/pcmanfm/.ai-phone-style-v1"
  fi
  if [ ! -f "$home/.config/.ai-phone-office-v1" ]; then
    if [ -d "$home/.config/libreoffice" ]; then mv "$home/.config/libreoffice" "$home/.config/libreoffice-before-desktop-fix"; fi
    touch "$home/.config/.ai-phone-office-v1"
  fi
  rm -f "$home/.config/libreoffice/4/.lock"
  mkdir -p "$home/Templates"
  ;;
desktop)
  cat > "$home/Desktop/Files.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=工作文件
Exec=pcmanfm /home/ai/Files
Icon=folder
Terminal=false
DESKTOP
  cat > "$home/Desktop/Calc.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=表格
Exec=libreoffice --calc
Icon=libreoffice-calc
Terminal=false
DESKTOP
  cat > "$home/Desktop/Writer.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=文档
Exec=libreoffice --writer
Icon=libreoffice-writer
Terminal=false
DESKTOP
  cat > "$home/Desktop/Impress.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=演示
Exec=libreoffice --impress
Icon=libreoffice-impress
Terminal=false
DESKTOP
  cat > "$home/Desktop/Draw.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=绘图
Exec=libreoffice --draw
Icon=libreoffice-draw
Terminal=false
DESKTOP
  cat > "$home/Desktop/Browser.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=浏览器
Exec=/usr/local/bin/ai-phone-browser
Icon=chromium
Terminal=false
DESKTOP
  rm -f "$home/Desktop/Clash.desktop"
  chmod +x "$home/Desktop/Files.desktop" "$home/Desktop/Calc.desktop" "$home/Desktop/Writer.desktop" "$home/Desktop/Impress.desktop" "$home/Desktop/Draw.desktop" "$home/Desktop/Browser.desktop"
  ;;
*) echo 'Unknown desktop home initialization stage' >&2; exit 1 ;;
esac
