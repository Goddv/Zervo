// Validate release artifacts before publishing them. This runs on the macOS
// release runner. Sparkle independently verifies the feed and delta in the app.
import CryptoKit
import Foundation

func require(_ condition: @autoclosure () -> Bool, _ error: String) throws {
    if !condition() { throw VerificationError.invalid(error) }
}

enum VerificationError: Error {
    case invalid(String)
}

func verify() throws {
    let args = CommandLine.arguments
    try require(args.count == 8, "expected feed, staging, archive, build, URL prefix, key and output")
    let feed = URL(fileURLWithPath: args[1])
    let staging = URL(fileURLWithPath: args[2], isDirectory: true)
    let output = URL(fileURLWithPath: args[7], isDirectory: true)
    guard let keyBytes = Data(base64Encoded: args[6]) else {
        throw VerificationError.invalid("invalid public key encoding")
    }
    let key = try Curve25519.Signing.PublicKey(rawRepresentation: keyBytes)

    let signedFeed = try Data(contentsOf: feed)
    let marker = Data("<!-- sparkle-signatures:\n".utf8)
    guard let range = signedFeed.range(of: marker, options: .backwards),
          let fields = String(data: signedFeed[range.upperBound...], encoding: .utf8),
          let signatureField = fields.split(separator: "\n").first(where: { $0.hasPrefix("edSignature:") }),
          let signature = Data(base64Encoded: signatureField.dropFirst("edSignature:".count).trimmingCharacters(in: .whitespaces)),
          let lengthField = fields.split(separator: "\n").first(where: { $0.hasPrefix("length:") }),
          let length = Int(lengthField.dropFirst("length:".count).trimmingCharacters(in: .whitespaces))
    else {
        throw VerificationError.invalid("missing feed signature")
    }
    let content = signedFeed[..<range.lowerBound]
    try require(content.count == length && key.isValidSignature(signature, for: content), "invalid feed signature")

    let xml = try XMLDocument(data: content, options: [.nodeLoadExternalEntitiesNever])
    let entries = try xml.nodes(forXPath: "/rss/channel/item")
    try require(entries.count == 1, "feed must describe exactly one release")
    let main = try entries[0].nodes(forXPath: "enclosure")
    try require(main.count == 1, "expected one update enclosure")
    let enclosure = main[0] as? XMLElement
    try require(enclosure?.attribute(forName: "url")?.stringValue == args[5] + args[3], "incorrect placeholder for the manual installer")
    let versions = try entries[0].nodes(forXPath: "*[local-name()='version']")
    try require(versions.first?.stringValue == args[4], "incorrect bundle version")
    let notes = try entries[0].nodes(forXPath: "description")
    try require(notes.count == 1 && notes[0].stringValue?.isEmpty == false, "missing signed release notes")

    var baselines: Set<String> = []
    for node in try entries[0].nodes(forXPath: "*[local-name()='deltas']/enclosure") {
        guard let delta = node as? XMLElement,
              let url = delta.attribute(forName: "url")?.stringValue,
              let from = delta.attribute(forName: "sparkle:deltaFrom")?.stringValue,
              let count = delta.attribute(forName: "length")?.stringValue.flatMap({ Int($0) }),
              let encodedSignature = delta.attribute(forName: "sparkle:edSignature")?.stringValue,
              let signature = Data(base64Encoded: encodedSignature)
        else {
            throw VerificationError.invalid("incomplete delta enclosure")
        }
        let name = String(url.dropFirst(args[5].count))
        try require(name.range(of: #"^Zervo[0-9.]+-[0-9.]+-(arm64|x86_64)\.delta$"#, options: .regularExpression) != nil &&
            url == args[5] + name && baselines.insert(from).inserted, "unexpected or duplicate delta")
        let patch = staging.appendingPathComponent(name)
        let data = try Data(contentsOf: patch, options: .mappedIfSafe)
        try require(data.count == count && key.isValidSignature(signature, for: data), "invalid delta signature for \(name)")
        try FileManager.default.copyItem(at: patch, to: output.appendingPathComponent(name))
    }
    print("Verified signed feed and \(baselines.count) incremental update(s).")
}

do {
    try verify()
} catch {
    FileHandle.standardError.write(Data("Cannot publish appcast: \(error)\n".utf8))
    exit(1)
}
