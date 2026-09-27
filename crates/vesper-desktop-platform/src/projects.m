// Included by macos.m. No document text, window titles or directory traversal.
static bool td_project_string(NSString *value, char *output, size_t size) {
    if (!value || !output || !size) return false;
    NSData *data = [value dataUsingEncoding:NSUTF8StringEncoding];
    if (!data || !data.length || data.length >= size) return false;
    memcpy(output, data.bytes, data.length);
    output[data.length] = '\0';
    return true;
}

bool td_project_application(char *identifier, size_t idSize, char *name, size_t nameSize, const char *selectedPath) {
    if (![NSThread isMainThread]) return false;
    NSString *bundleID = nil, *displayName = nil;
    if (selectedPath) {
        NSString *path = [NSString stringWithUTF8String:selectedPath];
        if (!path.isAbsolutePath || ![path.pathExtension isEqualToString:@"app"]) return false;
        NSURL *url = [NSURL fileURLWithPath:path];
        NSBundle *bundle = [NSBundle bundleWithURL:url];
        bundleID = bundle.bundleIdentifier;
        displayName = [bundle objectForInfoDictionaryKey:@"CFBundleDisplayName"];
        if (!displayName) displayName = [bundle objectForInfoDictionaryKey:@"CFBundleName"];
        if (!displayName) displayName = url.lastPathComponent.stringByDeletingPathExtension;
    } else {
        NSRunningApplication *app = NSWorkspace.sharedWorkspace.frontmostApplication;
        if (!app || app.processIdentifier == getpid()) return false;
        bundleID = app.bundleIdentifier;
        displayName = app.localizedName;
    }
    return td_project_string(bundleID, identifier, idSize) && td_project_string(displayName, name, nameSize);
}

bool td_project_document(const char *application, char *path, size_t size) {
    if (![NSThread isMainThread] || !application || !AXIsProcessTrusted()) return false;
    NSString *expected = [NSString stringWithUTF8String:application];
    // Folder routing starts with the native editor used by the owned fixture.
    // Other editors need a qualified document-context adapter, not title guesses.
    if (![expected isEqualToString:@"com.apple.TextEdit"]) return false;
    NSRunningApplication *app = NSWorkspace.sharedWorkspace.frontmostApplication;
    if (!app || ![app.bundleIdentifier isEqualToString:expected] || app.processIdentifier == getpid()) return false;
    pid_t pid = app.processIdentifier;
    AXUIElementRef axApp = AXUIElementCreateApplication(pid);
    AXUIElementSetMessagingTimeout(axApp, 0.03);
    CFTypeRef window = NULL, document = NULL;
    NSString *value = nil;
    if (AXUIElementCopyAttributeValue(axApp, kAXFocusedWindowAttribute, &window) == kAXErrorSuccess &&
        window && CFGetTypeID(window) == AXUIElementGetTypeID()) {
        AXUIElementSetMessagingTimeout((AXUIElementRef)window, 0.03);
        if (AXUIElementCopyAttributeValue((AXUIElementRef)window, kAXDocumentAttribute, &document) == kAXErrorSuccess && document) {
            NSURL *url = nil;
            if (CFGetTypeID(document) == CFStringGetTypeID()) url = [NSURL URLWithString:(__bridge NSString *)document];
            else if (CFGetTypeID(document) == CFURLGetTypeID()) url = (__bridge NSURL *)document;
            if (url.isFileURL && (!url.host.length || [url.host isEqualToString:@"localhost"]))
                value = url.path.stringByStandardizingPath.stringByResolvingSymlinksInPath;
        }
    }
    CFTypeRef currentWindow = NULL;
    bool sameWindow = AXUIElementCopyAttributeValue(axApp, kAXFocusedWindowAttribute, &currentWindow) == kAXErrorSuccess && currentWindow && window && CFEqual(currentWindow, window);
    if (currentWindow) CFRelease(currentWindow);
    if (document) CFRelease(document);
    if (window) CFRelease(window);
    CFRelease(axApp);
    if (!sameWindow || NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier != pid) return false;
    return td_project_string(value, path, size);
}
