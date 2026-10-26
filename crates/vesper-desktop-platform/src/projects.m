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

// Bounded walk from the focused element to its enclosing web area. Returns the
// http(s) host only; the path, query and fragment never leave this function.
static NSString *td_project_web_host(AXUIElementRef axApp, CFAbsoluteTime deadline) {
    CFTypeRef focused = NULL;
    if (AXUIElementCopyAttributeValue(axApp, kAXFocusedUIElementAttribute, &focused) != kAXErrorSuccess ||
        !focused || CFGetTypeID(focused) != AXUIElementGetTypeID()) {
        if (focused) CFRelease(focused);
        return nil;
    }
    NSString *host = nil;
    AXUIElementRef element = (AXUIElementRef)focused;  // owned
    NSArray *names = @[(__bridge NSString *)kAXRoleAttribute, (__bridge NSString *)kAXSubroleAttribute,
                       (__bridge NSString *)kAXParentAttribute, (__bridge NSString *)kAXURLAttribute];
    for (int depth = 0; element && depth < 48 && CFAbsoluteTimeGetCurrent() < deadline; depth++) {
        AXUIElementSetMessagingTimeout(element, 0.03);
        CFArrayRef values = NULL;
        if (AXUIElementCopyMultipleAttributeValues(element, (__bridge CFArrayRef)names, 0, &values) != kAXErrorSuccess || !values) break;
        NSArray *row = (__bridge_transfer NSArray *)values;
        id role = row.count > 0 ? row[0] : nil, subrole = row.count > 1 ? row[1] : nil;
        id parent = row.count > 2 ? row[2] : nil, url = row.count > 3 ? row[3] : nil;
        // Never read context from a secure field.
        if (depth == 0 && [subrole isKindOfClass:NSString.class] &&
            [subrole isEqualToString:(__bridge NSString *)kAXSecureTextFieldSubrole]) break;
        if ([role isKindOfClass:NSString.class] && [role isEqualToString:@"AXWebArea"]) {
            NSURL *page = nil;
            if ([url isKindOfClass:NSURL.class]) page = url;
            else if ([url isKindOfClass:NSString.class]) page = [NSURL URLWithString:url];
            NSString *scheme = page.scheme.lowercaseString;
            if (([scheme isEqualToString:@"https"] || [scheme isEqualToString:@"http"]) && page.host.length)
                host = page.host.lowercaseString;
            break;
        }
        CFRelease(element);
        element = NULL;
        if (parent && CFGetTypeID((__bridge CFTypeRef)parent) == AXUIElementGetTypeID())
            element = (AXUIElementRef)CFRetain((__bridge CFTypeRef)parent);
    }
    if (element) CFRelease(element);
    return host;
}

// Generic focused-window context for any frontmost app: the window's AXDocument
// file URL, or (for browsers, when asked) the focused web area's host. Returns
// 0 none, 1 file, 2 directory (lexical trailing slash), 3 web host, -1 failure.
int td_project_context(const char *application, bool web, char *output, size_t size) {
    if (![NSThread isMainThread] || !application || !AXIsProcessTrusted()) return -1;
    NSString *expected = [NSString stringWithUTF8String:application];
    NSRunningApplication *app = NSWorkspace.sharedWorkspace.frontmostApplication;
    if (!expected || !app || ![app.bundleIdentifier isEqualToString:expected] || app.processIdentifier == getpid()) return -1;
    pid_t pid = app.processIdentifier;
    CFAbsoluteTime deadline = CFAbsoluteTimeGetCurrent() + 0.06;
    AXUIElementRef axApp = AXUIElementCreateApplication(pid);
    AXUIElementSetMessagingTimeout(axApp, 0.03);
    CFTypeRef window = NULL, document = NULL;
    NSString *value = nil;
    int kind = 0;
    // Never read context while a secure (password) field has focus.
    CFTypeRef focused = NULL, subrole = NULL;
    if (AXUIElementCopyAttributeValue(axApp, kAXFocusedUIElementAttribute, &focused) == kAXErrorSuccess &&
        focused && CFGetTypeID(focused) == AXUIElementGetTypeID()) {
        AXUIElementSetMessagingTimeout((AXUIElementRef)focused, 0.03);
        AXUIElementCopyAttributeValue((AXUIElementRef)focused, kAXSubroleAttribute, &subrole);
    }
    bool secure = subrole && CFGetTypeID(subrole) == CFStringGetTypeID() &&
        CFEqual(subrole, kAXSecureTextFieldSubrole);
    if (subrole) CFRelease(subrole);
    if (focused) CFRelease(focused);
    if (secure) {
        CFRelease(axApp);
        return 0;
    }
    if (AXUIElementCopyAttributeValue(axApp, kAXFocusedWindowAttribute, &window) == kAXErrorSuccess &&
        window && CFGetTypeID(window) == AXUIElementGetTypeID()) {
        AXUIElementSetMessagingTimeout((AXUIElementRef)window, 0.03);
        if (AXUIElementCopyAttributeValue((AXUIElementRef)window, kAXDocumentAttribute, &document) == kAXErrorSuccess && document) {
            NSURL *url = nil;
            if (CFGetTypeID(document) == CFStringGetTypeID()) url = [NSURL URLWithString:(__bridge NSString *)document];
            else if (CFGetTypeID(document) == CFURLGetTypeID()) url = (__bridge NSURL *)document;
            // Lexical only: no symlink resolution or path standardization, both of
            // which consult the filesystem. The core rejects `.`/`..` components.
            NSString *scheme = url.scheme.lowercaseString;
            if (url.isFileURL && (!url.host.length || [url.host isEqualToString:@"localhost"]) && url.path.length) {
                value = url.path;
                // The fixed system symlinks, mapped lexically so folder rules chosen
                // through the (canonicalizing) picker still match.
                for (NSString *link in @[@"/tmp", @"/var", @"/etc"])
                    if ([value isEqualToString:link] || [value hasPrefix:[link stringByAppendingString:@"/"]]) {
                        value = [@"/private" stringByAppendingString:value];
                        break;
                    }
                kind = url.hasDirectoryPath ? 2 : 1;
            } else if (web && ([scheme isEqualToString:@"https"] || [scheme isEqualToString:@"http"]) && url.host.length) {
                // Some browsers expose the page as the window document: keep the host only.
                value = url.host.lowercaseString;
                kind = 3;
            }
        }
        if (!value && web) {
            value = td_project_web_host(axApp, deadline);
            if (value) kind = 3;
        }
    }
    CFTypeRef currentWindow = NULL;
    bool sameWindow = AXUIElementCopyAttributeValue(axApp, kAXFocusedWindowAttribute, &currentWindow) == kAXErrorSuccess && currentWindow && window && CFEqual(currentWindow, window);
    if (currentWindow) CFRelease(currentWindow);
    if (document) CFRelease(document);
    if (window) CFRelease(window);
    CFRelease(axApp);
    if (!sameWindow || NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier != pid) return -1;
    if (!kind) return 0;
    return td_project_string(value, output, size) ? kind : -1;
}
