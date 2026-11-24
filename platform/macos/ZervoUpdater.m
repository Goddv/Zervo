#import <Foundation/Foundation.h>
#import <Sparkle/Sparkle.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static NSString *const ZervoUpdaterDomain = @"io.zervo.updater";
typedef void (*Wake)(void *);

@interface ZervoUpdaterDelegate : NSObject <SPUUpdaterDelegate>
@property(nonatomic, strong) SPUStandardUpdaterController *controller;
@property(nonatomic, copy) NSString *status;
@property(nonatomic, strong, nullable) SUAppcastItem *updateItem;
@property(nonatomic) Wake wake;
@property(nonatomic) void *context;
- (void)notify;
@end

@implementation ZervoUpdaterDelegate

- (void)notify {
    if (self.wake != NULL) self.wake(self.context);
}

- (void)observeValueForKeyPath:(NSString *)keyPath ofObject:(id)object
                       change:(NSDictionary *)change context:(void *)context {
    [self notify];
}

- (NSString *)feedURLStringForUpdater:(SPUUpdater *)updater {
    // Read the signed bundle's feed URL directly, bypassing user defaults.
    return [NSBundle.mainBundle objectForInfoDictionaryKey:@"SUFeedURL"];
}

- (BOOL)updater:(SPUUpdater *)updater mayPerformUpdateCheck:(SPUUpdateCheck)check
          error:(NSError **)error {
    self.updateItem = nil;
    self.status = @"checking";
    [self notify];
    return YES;
}

- (BOOL)updater:(SPUUpdater *)updater shouldProceedWithUpdate:(SUAppcastItem *)item
    updateCheck:(SPUUpdateCheck)updateCheck error:(NSError **)error {
    if (item.isDeltaUpdate && [item.fileURL.scheme isEqualToString:@"https"]) return YES;
    self.updateItem = item;
    self.status = @"manual";
    [self notify];
    if (error != NULL && updateCheck == SPUUpdateCheckUpdates) {
        *error = [NSError errorWithDomain:ZervoUpdaterDomain code:1 userInfo:@{
            NSLocalizedDescriptionKey: @"This build has no compatible incremental update.",
            NSLocalizedRecoverySuggestionErrorKey: @"Get the latest installer from Zervo’s GitHub releases."
        }];
    }
    return NO;
}

- (void)updater:(SPUUpdater *)updater didFindValidUpdate:(SUAppcastItem *)item {
    self.updateItem = item;
    self.status = @"available";
    [self notify];
}

- (BOOL)updater:(SPUUpdater *)updater shouldDownloadReleaseNotesForUpdate:(SUAppcastItem *)item {
    return NO;
}

- (void)updater:(SPUUpdater *)updater willDownloadUpdate:(SUAppcastItem *)item
    withRequest:(NSMutableURLRequest *)request {
    self.status = @"downloading";
    [self notify];
}

- (void)updater:(SPUUpdater *)updater didExtractUpdate:(SUAppcastItem *)item {
    self.status = @"staging";
    [self notify];
}

- (BOOL)updater:(SPUUpdater *)updater willInstallUpdateOnQuit:(SUAppcastItem *)item
    immediateInstallationBlock:(void (^)(void))install {
    self.status = @"ready";
    [self notify];
    // Sparkle handles installation once the user quits normally.
    return NO;
}

- (void)updater:(SPUUpdater *)updater willInstallUpdate:(SUAppcastItem *)item {
    self.status = @"installing";
    [self notify];
}

- (void)updater:(SPUUpdater *)updater didFinishUpdateCycleForUpdateCheck:(SPUUpdateCheck)updateCheck
          error:(NSError *)error {
    if (error == nil) {
        if ([self.status isEqualToString:@"checking"]) self.status = @"idle";
    } else if ([error.domain isEqualToString:ZervoUpdaterDomain]) {
        self.status = @"manual";
    } else if ([error.domain isEqualToString:SUSparkleErrorDomain] && error.code == SUNoUpdateError) {
        NSInteger reason = [error.userInfo[SPUNoUpdateFoundReasonKey] integerValue];
        self.status = (reason == SPUNoUpdateFoundReasonOnLatestVersion ||
                       reason == SPUNoUpdateFoundReasonOnNewerThanLatestVersion) ? @"current" : @"no_compatible";
    } else {
        self.status = @"failed";
    }
    [self notify];
}

@end

// The Rust event loop owns the handle. All entry points and Sparkle delegate
// callbacks run on AppKit's main thread.
void *zervo_updater_start(Wake wake, void *context) {
    NSBundle *bundle = NSBundle.mainBundle;
    NSURL *feed = [NSURL URLWithString:[bundle objectForInfoDictionaryKey:@"SUFeedURL"] ?: @""];
    if (![feed.scheme isEqualToString:@"https"] ||
        ![[bundle objectForInfoDictionaryKey:@"SURequireSignedFeed"] boolValue] ||
        ![[bundle objectForInfoDictionaryKey:@"SUVerifyUpdateBeforeExtraction"] boolValue]) return NULL;
    ZervoUpdaterDelegate *bridge = [ZervoUpdaterDelegate new];
    bridge.status = @"idle";
    bridge.wake = wake;
    bridge.context = context;
    bridge.controller = [[SPUStandardUpdaterController alloc] initWithStartingUpdater:NO
                                                                     updaterDelegate:bridge
                                                                  userDriverDelegate:nil];
    NSError *error = nil;
    if (![bridge.controller.updater startUpdater:&error]) {
        NSLog(@"Zervo updater failed to start: %@", error);
        return NULL;
    }
    for (NSString *key in @[@"canCheckForUpdates", @"automaticallyChecksForUpdates", @"automaticallyDownloadsUpdates"]) {
        [bridge.controller.updater addObserver:bridge forKeyPath:key options:0 context:NULL];
    }
    return (__bridge_retained void *)bridge;
}

void zervo_updater_stop(void *handle) {
    ZervoUpdaterDelegate *bridge = (__bridge_transfer ZervoUpdaterDelegate *)handle;
    bridge.wake = NULL;
    for (NSString *key in @[@"canCheckForUpdates", @"automaticallyChecksForUpdates", @"automaticallyDownloadsUpdates"]) {
        [bridge.controller.updater removeObserver:bridge forKeyPath:key];
    }
}

void zervo_updater_check(void *handle) {
    ZervoUpdaterDelegate *bridge = (__bridge ZervoUpdaterDelegate *)handle;
    if (bridge.controller.updater.canCheckForUpdates) [bridge.controller checkForUpdates:nil];
}

void zervo_updater_set_automatic(void *handle, uint8_t enabled) {
    ZervoUpdaterDelegate *bridge = (__bridge ZervoUpdaterDelegate *)handle;
    SPUUpdater *updater = bridge.controller.updater;
    updater.automaticallyChecksForUpdates = enabled != 0;
    updater.automaticallyDownloadsUpdates = enabled != 0;
    [bridge notify];
}

char *zervo_updater_snapshot(void *handle) {
    ZervoUpdaterDelegate *bridge = (__bridge ZervoUpdaterDelegate *)handle;
    SPUUpdater *updater = bridge.controller.updater;
    SUAppcastItem *release = bridge.updateItem;
    NSString *notes = [release.itemDescriptionFormat isEqualToString:@"plain-text"] ? release.itemDescription : nil;
    if (notes.length > 12000) notes = [notes substringToIndex:12000];
    NSDictionary *values = @{
        @"status": bridge.status,
        @"can_check": @(updater.canCheckForUpdates),
        @"automatic": [NSNumber numberWithBool:(updater.automaticallyChecksForUpdates && updater.automaticallyDownloadsUpdates)],
        @"version": release.displayVersionString ?: @"",
        @"notes": notes ?: @"",
        @"bytes": @(release.isDeltaUpdate ? release.contentLength : 0)
    };
    NSData *data = [NSJSONSerialization dataWithJSONObject:values options:0 error:NULL];
    NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
    return json != nil ? strdup(json.UTF8String) : NULL;
}
