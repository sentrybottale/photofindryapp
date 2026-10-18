#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
[[ "$(uname -s)" == Darwin ]] || { echo 'The companion tools currently require macOS.' >&2; exit 1; }
if [[ $# == 0 ]]; then
  target_dir="$project_dir/engine/target/debug"
  mkdir -p "$target_dir"
elif [[ $# == 2 && "$1" == --output && "$2" == /* ]]; then
  target_dir="$2"
  python3 - "$target_dir" <<'PYOUTPUT'
import os,pathlib,stat,sys
path=pathlib.Path(sys.argv[1]);info=path.lstat()
if not stat.S_ISDIR(info.st_mode) or path.is_symlink() or info.st_uid!=os.getuid() or stat.S_IMODE(info.st_mode)!=0o700:
 raise SystemExit('Explicit helper output must already be an owner-only real directory.')
PYOUTPUT
else
  echo 'Usage: bash scripts/build-companion-tools.sh [--output /absolute/owner-only-directory]' >&2
  exit 2
fi
export MACOSX_DEPLOYMENT_TARGET=14.0
native_target="$(uname -m)-apple-macosx14.0"
swift_flags=(-target "$native_target" -O
  -debug-prefix-map "$project_dir=/photofindry"
  -file-prefix-map "$project_dir=/photofindry")
swiftc "${swift_flags[@]}" -framework AppKit -framework UniformTypeIdentifiers \
  "$project_dir/scripts/companion-picker.swift" -o "$target_dir/CompanionPicker"
codesign --force --sign - "$target_dir/CompanionPicker"
swiftc "${swift_flags[@]}" -framework AppKit -framework CoreText -framework SwiftUI \
  "$project_dir/scripts/companion-glyphs.swift" \
  "$project_dir/Sources/PhotoRecall/EmojiCatalog.swift" \
  "$project_dir/Sources/PhotoRecall/EmojiGlyphCache.swift" \
  "$project_dir/Sources/PhotoRecall/EmojiPixelRaster.swift" \
  -o "$target_dir/CompanionGlyphs"
codesign --force --sign - "$target_dir/CompanionGlyphs"
swiftc "${swift_flags[@]}" -framework AppKit -framework CryptoKit \
  "$project_dir/scripts/companion-downloads.swift" \
  "$project_dir/Sources/PhotoRecall/RerankerDownload.swift" \
  "$project_dir/Sources/PhotoRecall/ClefFlashDownload.swift" \
  -o "$target_dir/CompanionDownloads"
codesign --force --sign - "$target_dir/CompanionDownloads"
swiftc "${swift_flags[@]}" -parse-as-library -framework AppKit -framework CoreServices \
  "$project_dir/scripts/companion-sources.swift" \
  "$project_dir/Sources/PhotoRecall/SourceVolumeMonitor.swift" \
  -o "$target_dir/CompanionSources"
codesign --force --sign - "$target_dir/CompanionSources"
swiftc "${swift_flags[@]}" -parse-as-library -framework AppKit -framework SwiftUI -framework AVFoundation -framework Vision \
  "$project_dir/scripts/companion-labels.swift" \
  "$project_dir/Sources/PhotoRecall/SourceLabels.swift" \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker "$project_dir/Resources/CompanionLabels-Info.plist" \
  -o "$target_dir/CompanionLabels"
codesign --force --sign - "$target_dir/CompanionLabels"
swiftc "${swift_flags[@]}" -parse-as-library -framework AppKit -framework ImageIO -framework UniformTypeIdentifiers -framework CryptoKit \
  "$project_dir/scripts/companion-feedback-image.swift" \
  "$project_dir/Sources/PhotoRecall/FeedbackAttachment.swift" \
  -o "$target_dir/CompanionFeedbackImage"
codesign --force --sign - "$target_dir/CompanionFeedbackImage"
swiftc "${swift_flags[@]}" -framework AppKit -framework CoreServices \
  "$project_dir/scripts/companion-launcher.swift" -o "$target_dir/CompanionLauncher"
codesign --force --sign - "$target_dir/CompanionLauncher"
# Give native dialogs a stable application identity and a recognisable owner.
# These small local UI helpers do not host the browser interface or an updater.
for tool in CompanionPicker CompanionDownloads CompanionLabels CompanionLauncher; do
  bundle="$target_dir/$tool.app"
  mkdir -p "$bundle/Contents/MacOS"
  cp "$target_dir/$tool" "$bundle/Contents/MacOS/$tool"
  python3 - "$bundle" "$tool" "$project_dir/Resources/Info.plist" <<'PY'
import pathlib,plistlib,sys
bundle=pathlib.Path(sys.argv[1]);tool=sys.argv[2]
product=plistlib.loads(pathlib.Path(sys.argv[3]).read_bytes())
versions={key:product.get(key) for key in ('CFBundleShortVersionString','CFBundleVersion')}
if any(not isinstance(value,str) or not value or len(value)>128 for value in versions.values()):
 raise SystemExit('The product version/build metadata is missing or invalid.')
names={'CompanionPicker':('picker','PhotoFindry file chooser'),'CompanionDownloads':('downloads','PhotoFindry model downloads'),'CompanionLabels':('labels','PhotoFindry archive scanner'),'CompanionLauncher':('launcher','Open PhotoFindry')}
suffix,name=names[tool]
info={'CFBundleIdentifier':'com.sentrybottale.photorecall.companion-'+suffix,'CFBundleName':name,'CFBundleDisplayName':name,'CFBundleExecutable':tool,'CFBundlePackageType':'APPL','LSUIElement':True,'LSMinimumSystemVersion':'14.0'}
info.update(versions)
if tool=='CompanionLabels':info['NSCameraUsageDescription']='Scan a PhotoFindry archive label locally. Camera frames are never saved or sent.'
if tool=='CompanionLauncher':info['CFBundleURLTypes']=[{'CFBundleURLName':'PhotoFindry local launch','CFBundleURLSchemes':['photofindry-local'],'CFBundleTypeRole':'Viewer'},{'CFBundleURLName':'PhotoFindry archive navigation','CFBundleURLSchemes':['photofindry'],'CFBundleTypeRole':'Viewer'}]
with (bundle/'Contents/Info.plist').open('wb') as f:plistlib.dump(info,f)
PY
  codesign --force --sign - "$bundle"
done
