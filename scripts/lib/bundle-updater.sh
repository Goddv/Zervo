# Include the verified-update bridge before signing the outer app.
zervo_bundle_updater() {
    local framework_source="$1" app="$2" feed="$3" public_key="$4" arch="$5"
    local frameworks="$2/Contents/Frameworks" resources="$2/Contents/Resources"
    local plist="$2/Contents/Info.plist" component
    mkdir -p "$frameworks"
    ditto "$framework_source/Sparkle.framework" "$frameworks/Sparkle.framework"
    cp "$framework_source/Sparkle-LICENSE" "$resources/Sparkle-LICENSE"
    xcrun clang -arch "$arch" -mmacosx-version-min=12.0 -fobjc-arc -dynamiclib \
        -F "$frameworks" -framework Foundation -framework Sparkle -framework UserNotifications \
        -Wl,-rpath,@loader_path -install_name '@rpath/ZervoUpdater.dylib' \
        platform/macos/ZervoUpdater.m -o "$frameworks/ZervoUpdater.dylib"

    plutil -insert SUFeedURL -string "$feed" "$plist"
    plutil -insert SUPublicEDKey -string "$public_key" "$plist"
    plutil -insert SUEnableAutomaticChecks -bool YES "$plist"
    plutil -insert SUAllowsAutomaticUpdates -bool YES "$plist"
    plutil -insert SUAutomaticallyUpdate -bool YES "$plist"
    plutil -insert SUEnableSystemProfiling -bool NO "$plist"
    plutil -insert SUEnableJavaScript -bool NO "$plist"
    plutil -insert SUVerifyUpdateBeforeExtraction -bool YES "$plist"
    plutil -insert SURequireSignedFeed -bool YES "$plist"
    plutil -insert SUSignedFeedFailureExpirationInterval -float 0 "$plist"
    # Updates-enabled builds carry Sparkle's supported minimum macOS.
    local minimum
    minimum="$(plutil -extract LSMinimumSystemVersion raw "$plist")"
    if [ "${minimum%%.*}" -lt 12 ]; then
        plutil -replace LSMinimumSystemVersion -string 12.0 "$plist"
    fi

    while IFS= read -r -d '' component; do
        sign_or_fail "$component"
    done < <(find "$frameworks/Sparkle.framework" -type d \( -name '*.app' -o -name '*.xpc' \) -depth -print0)
    sign_or_fail "$frameworks/Sparkle.framework/Versions/Current/Autoupdate"
    sign_or_fail "$frameworks/Sparkle.framework"
    sign_or_fail "$frameworks/ZervoUpdater.dylib"
    codesign --verify --deep --strict "$frameworks/Sparkle.framework"
}
