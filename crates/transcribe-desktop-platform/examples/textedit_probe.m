// Opt-in real TextEdit qualification using only a newly-created temporary fixture.
// Build with clang -fobjc-arc -fblocks and AppKit/ApplicationServices/Carbon frameworks.
#import "../src/macos.m"
static bool delivered;
static void received(uint64_t token, const char *before, const char *after) {
    (void)token;
    delivered = before && after && strcmp(before,"read me 🧑🏽‍💻")==0 && strcmp(after,"README 🧑🏽‍💻")==0;
}
static void pump(double seconds) {
    NSDate *end=[NSDate dateWithTimeIntervalSinceNow:seconds];
    while (end.timeIntervalSinceNow>0) [NSRunLoop.mainRunLoop runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.02]];
}
static void cleanupFixture(void) { @autoreleasepool { td_observation_cancel(); td_target_clear(0); restoreClipboard(); } }
int main(int argc, const char **argv) {
    atexit(cleanupFixture);
    @autoreleasepool {
        [NSApplication sharedApplication];
        printf("{\"accessibility\":%s,\"eventPosting\":%s}\n", AXIsProcessTrusted()?"true":"false", CGPreflightPostEventAccess()?"true":"false");
        if (!AXIsProcessTrusted() || !CGPreflightPostEventAccess()) return 77;
        if (argc<2) return 0;
        bool axFixture=strcmp(argv[1],"--ax-fixture")==0;
        if (!axFixture && strcmp(argv[1],"--fixture")!=0) return 0;
        NSString *prefix=@"BEGIN fixture left guard | ", *suffix=@" | right guard END fixture";
        NSString *path=[NSTemporaryDirectory() stringByAppendingPathComponent:[NSString stringWithFormat:@"transcribe-observer-%@.txt",NSUUID.UUID.UUIDString]];
        [[prefix stringByAppendingString:suffix] writeToFile:path atomically:YES encoding:NSUTF8StringEncoding error:nil];
        NSURL *url=[NSURL fileURLWithPath:path];
        NSWorkspaceOpenConfiguration *configuration=[NSWorkspaceOpenConfiguration configuration];
        configuration.activates=YES;
        __block bool opened=false;
        [NSWorkspace.sharedWorkspace openURLs:@[url] withApplicationAtURL:[NSURL fileURLWithPath:@"/System/Applications/TextEdit.app"] configuration:configuration completionHandler:^(NSRunningApplication *app,NSError *error) { opened=app!=nil && error==nil; }];
        pump(2);
        if (!opened) { puts("{\"failure\":\"fixture_open\"}"); return 1; }
        NSRunningApplication *front=NSWorkspace.sharedWorkspace.frontmostApplication;
        if (![front.bundleIdentifier isEqual:@"com.apple.TextEdit"]) return 2;
        AXUIElementRef field=focused(front.processIdentifier);
        id window=field ? attribute(field,kAXWindowAttribute):nil;
        NSString *document=window?attribute((__bridge AXUIElementRef)window,kAXDocumentAttribute):nil;
        // This opt-in harness verifies its OWN URL before touching content. The
        // production observer never reads document URLs or window titles.
        if (![document isKindOfClass:NSString.class] || ![[[NSURL URLWithString:document].URLByResolvingSymlinksInPath path] isEqual:url.URLByResolvingSymlinksInPath.path]) { printf("{\"failure\":\"fixture_identity\",\"field\":%s,\"window\":%s,\"documentString\":%s}\n", field?"true":"false",window?"true":"false",[document isKindOfClass:NSString.class]?"true":"false"); if(field)CFRelease(field); return 3; }
        CFRange selected=CFRangeMake(prefix.length,0);
        AXValueRef range=AXValueCreate(kAXValueCFRangeType,&selected);
        AXError selectError=AXUIElementSetAttributeValue(field,kAXSelectedTextRangeAttribute,range);CFRelease(range);
        Boolean valueWritable=false,selectedWritable=false;
        AXError valueError=AXUIElementIsAttributeSettable(field,kAXValueAttribute,&valueWritable);
        AXError selectedError=AXUIElementIsAttributeSettable(field,kAXSelectedTextAttribute,&selectedWritable);
        printf("{\"textArea\":%s,\"enabled\":%s,\"enabledPresent\":%s,\"valueWritable\":%s,\"valueError\":%d,\"selectedWritable\":%s,\"selectedError\":%d}\n",
            [attribute(field,kAXRoleAttribute) isEqual:(__bridge NSString*)kAXTextAreaRole]?"true":"false",
            [attribute(field,kAXEnabledAttribute) boolValue]?"true":"false",attribute(field,kAXEnabledAttribute)?"true":"false",valueWritable?"true":"false",valueError,selectedWritable?"true":"false",selectedError);
        int capture=td_target_capture(1);
        AXUIElementRef app=AXUIElementCreateApplication(front.processIdentifier);
        bool plain=plainTextEdit(app);CFRelease(app);
        printf("{\"select\":%d,\"capture\":%d,\"plainText\":%s,\"permissionBits\":%u}\n",selectError,capture,plain?"true":"false",td_permissions(false));
        if (selectError!=kAXErrorSuccess || capture!=0 || !plain) { CFRelease(field);return 4; }
        td_observation_arm(1,received);
        printf("{\"combinedFlags\":%llu,\"hardwareFlags\":%llu}\n",(unsigned long long)CGEventSourceFlagsState(kCGEventSourceStateCombinedSessionState),(unsigned long long)CGEventSourceFlagsState(kCGEventSourceStateHIDSystemState));
        int paste=1;
        if (axFixture) {
            beginObservation(1,@"read me 🧑🏽‍💻");
            paste=AXUIElementSetAttributeValue(field,kAXSelectedTextAttribute,CFSTR("read me 🧑🏽‍💻"));
            td_target_clear(1);
        } else {
            for (int attempt=0; attempt<120 && paste==1; attempt++) { paste=td_paste(1,"read me 🧑🏽‍💻"); if(paste==1)pump(0.025); }
        }
        printf("{\"afterDispatchReason\":\"%s\",\"registrationIndex\":%d,\"registrationError\":%d}\n",observationReason,observationRegistrationIndex,observationRegistrationError);
        pump(0.5);
        printf("{\"paste\":%d,\"acknowledged\":%s,\"receiptActive\":%s}\n",paste,acknowledged?"true":"false",observationToken?"true":"false");
        printf("{\"observerReason\":\"%s\",\"registrationIndex\":%d,\"registrationError\":%d}\n",observationReason,observationRegistrationIndex,observationRegistrationError);
        if(paste!=0 || !observationToken || !acknowledged) { td_observation_cancel(); CFRelease(field);return 5; }
        const char *scenario=argc>2?argv[2]:"correction";
        if (strcmp(scenario,"cancel")==0) {
            td_observation_cancel();pump(0.4);
            printf("{\"scenario\":\"cancel\",\"rejected\":%s}\n",!delivered && !observationToken?"true":"false");
            CFRelease(field);return !delivered && !observationToken?0:7;
        }
        selected=strcmp(scenario,"outside")==0 ? CFRangeMake(0,1) : CFRangeMake(prefix.length,[@"read me 🧑🏽‍💻" length]);
        range=AXValueCreate(kAXValueCFRangeType,&selected);
        AXError selection=AXUIElementSetAttributeValue(field,kAXSelectedTextRangeAttribute,range);CFRelease(range);
        AXError edit=AXUIElementSetAttributeValue(field,kAXSelectedTextAttribute,CFSTR("README 🧑🏽‍💻"));
        pump(0.7);
        printf("{\"replacementSelection\":%d,\"replacement\":%d,\"candidateMatched\":%s,\"boundedReads\":%llu,\"largestUtf16Read\":%llu}\n",selection,edit,delivered?"true":"false",(unsigned long long)observationReads,(unsigned long long)observationMaxRead);
        td_observation_cancel();td_target_clear(0);restoreClipboard();CFRelease(field);
        // Leave this uniquely named authored document available for inspection.
        bool expected = strcmp(scenario,"outside")==0 ? !delivered && !observationToken : delivered;
        return expected && observationMaxRead<=8256 ? 0 : 6;
    }
}
