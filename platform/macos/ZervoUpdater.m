#import <Foundation/Foundation.h>
#import <Sparkle/Sparkle.h>
#import <UserNotifications/UserNotifications.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static NSString *const ZervoUpdaterDomain = @"io.zervo.updater";
static NSString *const NotificationIdentifier = @"app.zervo.browser-update";
static NSString *const NotificationsEnabledKey = @"ZervoUpdateNotificationsEnabled";
static NSString *const LastNoticeKey = @"ZervoLastUpdateNotice";
static NSString *const ReminderDateKey = @"ZervoUpdateReminderDate";
static NSString *const ReminderBaseKey = @"ZervoUpdateReminderBase";
static NSString *const ReminderTargetKey = @"ZervoUpdateReminderTarget";
static const NSTimeInterval ReminderInterval = 24 * 60 * 60;
static NSArray<NSString *> *ObservedKeys(void) {
    return @[@"canCheckForUpdates", @"automaticallyChecksForUpdates", @"automaticallyDownloadsUpdates", @"updateCheckInterval"];
}
typedef void (*Wake)(void *);

@interface ZervoUpdaterDelegate : NSObject <SPUUpdaterDelegate, UNUserNotificationCenterDelegate>
@property(nonatomic, strong) SPUStandardUpdaterController *controller;
@property(nonatomic, copy) NSString *status;
@property(nonatomic, strong, nullable) SUAppcastItem *updateItem;
@property(nonatomic) Wake wake;
@property(nonatomic) void *context;
@property(nonatomic) BOOL openUpdates;
@property(nonatomic) BOOL notificationsBlocked;
@property(nonatomic, copy) NSString *noticeVersion;
- (void)notify;
- (void)noticeForUpdate:(SUAppcastItem *)item;
- (void)armReminder;
- (void)setSystemNotifications:(BOOL)enabled;
- (void)remindTomorrow;
@end

@implementation ZervoUpdaterDelegate

- (void)notify {
    if (self.wake != NULL) self.wake(self.context);
}

- (NSDate *)reminderDate {
    return [NSUserDefaults.standardUserDefaults objectForKey:ReminderDateKey];
}

- (void)clearReminder {
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
    for (NSString *key in @[ReminderDateKey, ReminderBaseKey, ReminderTargetKey]) {
        [defaults removeObjectForKey:key];
    }
    [UNUserNotificationCenter.currentNotificationCenter removePendingNotificationRequestsWithIdentifiers:@[NotificationIdentifier]];
}

- (void)sendSystemNotice:(SUAppcastItem *)item after:(NSTimeInterval)delay {
    if (![NSUserDefaults.standardUserDefaults boolForKey:NotificationsEnabledKey]) return;
    UNMutableNotificationContent *content = [UNMutableNotificationContent new];
    content.title = @"Zervo update available";
    content.body = [NSString stringWithFormat:@"Version %@ is available. Open Zervo for update details.", item.displayVersionString];
    UNTimeIntervalNotificationTrigger *trigger = [UNTimeIntervalNotificationTrigger triggerWithTimeInterval:MAX(delay, 1) repeats:NO];
    UNNotificationRequest *request = [UNNotificationRequest requestWithIdentifier:NotificationIdentifier content:content trigger:trigger];
    [UNUserNotificationCenter.currentNotificationCenter addNotificationRequest:request withCompletionHandler:^(NSError *error) {
        if (error != nil) NSLog(@"Zervo update notification failed: %@", error.localizedDescription);
    }];
}

- (void)noticeForUpdate:(SUAppcastItem *)item {
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
    NSDate *due = self.reminderDate;
    BOOL reminding = [item.versionString isEqualToString:[defaults stringForKey:ReminderTargetKey]];
    if (due != nil && reminding && due.timeIntervalSinceNow > 0) return;
    if (due != nil) [self clearReminder];
    if (item.versionString.length == 0 ||
        (!reminding && [item.versionString isEqualToString:[defaults stringForKey:LastNoticeKey]])) return;
    [defaults setObject:item.versionString forKey:LastNoticeKey];
    self.noticeVersion = item.displayVersionString;
    // A scheduled OS reminder may already have been delivered at its deadline.
    if (!reminding) [self sendSystemNotice:item after:1];
    [self notify];
}

- (void)armReminder {
    NSDate *due = self.reminderDate;
    if (due == nil) return;
    NSString *base = [NSUserDefaults.standardUserDefaults stringForKey:ReminderBaseKey];
    if (![base isEqualToString:[NSBundle.mainBundle objectForInfoDictionaryKey:@"CFBundleVersion"]]) {
        [self clearReminder];
        [UNUserNotificationCenter.currentNotificationCenter removeDeliveredNotificationsWithIdentifiers:@[NotificationIdentifier]];
        return;
    }
    NSTimeInterval delay = MIN(ReminderInterval, MAX(1, due.timeIntervalSinceNow));
    __weak ZervoUpdaterDelegate *weakSelf = self;
    dispatch_after(dispatch_walltime(NULL, (int64_t)(delay * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        ZervoUpdaterDelegate *bridge = weakSelf;
        if (![bridge.reminderDate isEqualToDate:due]) return;
        if (bridge.updateItem != nil) {
            [bridge noticeForUpdate:bridge.updateItem];
        } else if (!bridge.controller.updater.sessionInProgress) {
            // A persisted reminder authorizes a fresh check after relaunch.
            [bridge.controller.updater checkForUpdateInformation];
        }
    });
}

- (void)remindTomorrow {
    SUAppcastItem *item = self.updateItem;
    if (item.versionString.length == 0) return;
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
    [defaults setObject:[NSDate dateWithTimeIntervalSinceNow:ReminderInterval] forKey:ReminderDateKey];
    [defaults setObject:[NSBundle.mainBundle objectForInfoDictionaryKey:@"CFBundleVersion"] forKey:ReminderBaseKey];
    [defaults setObject:item.versionString forKey:ReminderTargetKey];
    [UNUserNotificationCenter.currentNotificationCenter removeDeliveredNotificationsWithIdentifiers:@[NotificationIdentifier]];
    [self sendSystemNotice:item after:ReminderInterval];
    [self armReminder];
    [self notify];
}

- (void)setSystemNotifications:(BOOL)enabled {
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
    UNUserNotificationCenter *center = UNUserNotificationCenter.currentNotificationCenter;
    if (!enabled) {
        [defaults removeObjectForKey:NotificationsEnabledKey];
        [center removePendingNotificationRequestsWithIdentifiers:@[NotificationIdentifier]];
        [center removeDeliveredNotificationsWithIdentifiers:@[NotificationIdentifier]];
        self.notificationsBlocked = NO;
        [self notify];
        return;
    }
    __weak ZervoUpdaterDelegate *weakSelf = self;
    [center requestAuthorizationWithOptions:UNAuthorizationOptionAlert completionHandler:^(BOOL allowed, NSError *error) {
        dispatch_async(dispatch_get_main_queue(), ^{
            ZervoUpdaterDelegate *bridge = weakSelf;
            if (bridge == nil) return;
            [defaults setBool:allowed forKey:NotificationsEnabledKey];
            bridge.notificationsBlocked = !allowed;
            if (allowed && bridge.updateItem != nil) {
                [bridge sendSystemNotice:bridge.updateItem after:bridge.reminderDate.timeIntervalSinceNow];
            }
            [bridge notify];
        });
    }];
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
    didReceiveNotificationResponse:(UNNotificationResponse *)response
    withCompletionHandler:(void (^)(void))completion {
    if ([response.notification.request.identifier isEqualToString:NotificationIdentifier] &&
        [response.actionIdentifier isEqualToString:UNNotificationDefaultActionIdentifier]) {
        dispatch_async(dispatch_get_main_queue(), ^{
            self.openUpdates = YES;
            [self notify];
        });
    }
    completion();
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
    [self noticeForUpdate:item];
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
    [self noticeForUpdate:item];
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
    [self clearReminder];
    [UNUserNotificationCenter.currentNotificationCenter removeDeliveredNotificationsWithIdentifiers:@[NotificationIdentifier]];
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
        if ([self.status isEqualToString:@"current"]) {
            [self clearReminder];
            [UNUserNotificationCenter.currentNotificationCenter removeDeliveredNotificationsWithIdentifiers:@[NotificationIdentifier]];
        }
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
    for (NSString *key in ObservedKeys()) {
        [bridge.controller.updater addObserver:bridge forKeyPath:key options:0 context:NULL];
    }
    UNUserNotificationCenter.currentNotificationCenter.delegate = bridge;
    [bridge armReminder];
    return (__bridge_retained void *)bridge;
}

void zervo_updater_stop(void *handle) {
    ZervoUpdaterDelegate *bridge = (__bridge_transfer ZervoUpdaterDelegate *)handle;
    bridge.wake = NULL;
    UNUserNotificationCenter *center = UNUserNotificationCenter.currentNotificationCenter;
    if (center.delegate == bridge) center.delegate = nil;
    for (NSString *key in ObservedKeys()) {
        [bridge.controller.updater removeObserver:bridge forKeyPath:key];
    }
}

void zervo_updater_check(void *handle) {
    ZervoUpdaterDelegate *bridge = (__bridge ZervoUpdaterDelegate *)handle;
    if (bridge.controller.updater.canCheckForUpdates) [bridge.controller checkForUpdates:nil];
}

// Operation values match src/updates/macos.rs.
void zervo_updater_configure(void *handle, uint8_t operation, uint32_t value) {
    ZervoUpdaterDelegate *bridge = (__bridge ZervoUpdaterDelegate *)handle;
    SPUUpdater *updater = bridge.controller.updater;
    switch (operation) {
        case 0: updater.automaticallyChecksForUpdates = value != 0; break;
        case 1: updater.automaticallyDownloadsUpdates = value != 0; break;
        case 2: updater.updateCheckInterval = MAX(3600, MIN(604800, value)); break;
        case 3: [bridge setSystemNotifications:value != 0]; break;
        case 4: [bridge remindTomorrow]; break;
    }
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
        @"automatic_checks": @(updater.automaticallyChecksForUpdates),
        @"automatic_install": @(updater.automaticallyDownloadsUpdates),
        @"check_interval": @((uint32_t)updater.updateCheckInterval),
        @"system_notifications": @([NSUserDefaults.standardUserDefaults boolForKey:NotificationsEnabledKey]),
        @"notifications_blocked": @(bridge.notificationsBlocked),
        @"reminder_scheduled": [NSNumber numberWithBool:(bridge.reminderDate.timeIntervalSinceNow > 0)],
        @"notice_version": bridge.noticeVersion ?: @"",
        @"open_updates": @(bridge.openUpdates),
        @"version": release.displayVersionString ?: @"",
        @"notes": notes ?: @"",
        @"bytes": @(release.isDeltaUpdate ? release.contentLength : 0)
    };
    bridge.noticeVersion = nil;
    bridge.openUpdates = NO;
    NSData *data = [NSJSONSerialization dataWithJSONObject:values options:0 error:NULL];
    NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
    return json != nil ? strdup(json.UTF8String) : NULL;
}
