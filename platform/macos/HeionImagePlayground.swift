import AppKit
import ImageIO
import ImagePlayground
import UniformTypeIdentifiers

public typealias Completion = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int) -> Void

@MainActor
private var active: WallpaperPlayground?

@MainActor
private final class WallpaperPlayground: NSObject, ImagePlaygroundViewController.Delegate {
    let parent: NSWindow
    let controller = ImagePlaygroundViewController()
    let completion: Completion
    let context: UnsafeMutableRawPointer?
    lazy var sheet = NSWindow(contentViewController: controller)
    private var completed = false

    init(parent: NSWindow, context: UnsafeMutableRawPointer?, completion: @escaping Completion) {
        self.parent = parent
        self.context = context
        self.completion = completion
    }

    func present() {
        controller.delegate = self
        sheet.setContentSize(NSSize(width: 680, height: 650))
        parent.beginSheet(sheet) { [weak self] _ in self?.finish(nil) }
    }

    func imagePlaygroundViewController(
        _ imagePlaygroundViewController: ImagePlaygroundViewController,
        didCreateImageAt imageURL: URL
    ) {
        // Export while the system's temporary URL is valid. ImageIO also
        // normalizes orientation and keeps the upload below texture limits.
        guard let source = CGImageSourceCreateWithURL(imageURL as CFURL, nil),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: 3072,
              ] as CFDictionary)
        else {
            finish(Data())
            return
        }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil) else {
            finish(Data())
            return
        }
        CGImageDestinationAddImage(destination, image, [
            kCGImageDestinationLossyCompressionQuality: 0.92,
        ] as CFDictionary)
        finish(CGImageDestinationFinalize(destination) ? data as Data : Data())
    }

    func imagePlaygroundViewControllerDidCancel(_ imagePlaygroundViewController: ImagePlaygroundViewController) {
        finish(nil)
    }

    private func finish(_ data: Data?) {
        guard !completed else { return }
        completed = true
        controller.delegate = nil
        if parent.attachedSheet === sheet {
            parent.endSheet(sheet)
        }
        if let data, !data.isEmpty {
            data.withUnsafeBytes { bytes in
                completion(context, bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count)
            }
        } else {
            // Zero means cancellation. A negative length means an export error.
            completion(context, nil, data == nil ? 0 : -1)
        }
        active = nil
    }
}

@MainActor
@_cdecl("heion_playground_available")
public func heionPlaygroundAvailable() -> Bool {
    ImagePlaygroundViewController.isAvailable
}

@MainActor
@_cdecl("heion_playground_open")
public func heionPlaygroundOpen(
    _ window: UnsafeMutableRawPointer,
    _ context: UnsafeMutableRawPointer?,
    _ completion: @escaping Completion
) -> Bool {
    guard ImagePlaygroundViewController.isAvailable, active == nil else { return false }
    let parent = Unmanaged<NSWindow>.fromOpaque(window).takeUnretainedValue()
    guard parent.attachedSheet == nil else { return false }
    let playground = WallpaperPlayground(parent: parent, context: context, completion: completion)
    active = playground
    playground.present()
    return true
}
