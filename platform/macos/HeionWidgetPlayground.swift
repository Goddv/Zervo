import AVFoundation
import Foundation
import FoundationModels
@preconcurrency import Speech

public typealias Completion = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int) -> Void

@Generable
private enum WidgetSize: String, Codable {
    case auto, small, pill, square, wide, large
}

@Generable
private enum ComponentKind: String, Codable {
    case text, bullets, checklist, note, links, metric, counter, timer, date, clock, image
}

@Generable
private struct WidgetLink: Codable {
    var label: String
    var url: String
}

@Generable
private struct WidgetComponent: Codable {
    var kind: ComponentKind

    @Guide(description: "Short label for a note, metric, counter, timer, date or clock. Omit it for other components.")
    var label: String?

    @Guide(description: "For image, a short subject to search online or an explicit HTTPS image URL copied from the user. For text, note or metric, displayed content. For date, YYYY-MM-DD. For clock, the IANA zone for that city, such as Europe/Paris, Europe/Kyiv or America/New_York. Use local only for an explicitly local clock.")
    var text: String?

    @Guide(description: "For bullets and checklist only, up to eight concise lines in the desired order.")
    var items: [String]?

    @Guide(description: "For links only, up to eight explicit HTTP(S) addresses copied from the request and their short labels.")
    var links: [WidgetLink]?

    @Guide(description: "For a counter, its starting integer. For a timer, its duration in minutes from 1 to 1440.")
    var value: Int?

    @Guide(description: "For a counter with a progress target, a positive integer goal, otherwise omit.")
    var goal: Int?

    @Guide(description: "For a counter, the positive integer change per tap. Defaults to one.")
    var step: Int?

    mutating func normalize(addresses: Set<URL>) -> Bool {
        if let label { self.label = clean(label, limit: 64) }
        if let text, kind != .image {
            self.text = String(text.trimmingCharacters(in: .whitespacesAndNewlines).prefix(500))
        }
        switch kind {
        case .image:
            guard let text else { return false }
            if text.contains("://") {
                guard let url = webURL(text), url.scheme == "https", addresses.contains(url) else {
                    return false
                }
                self.text = url.absoluteString
            } else {
                self.text = clean(text, limit: 120)
            }
            return self.text?.isEmpty == false
        case .text:
            return text?.isEmpty == false
        case .bullets, .checklist:
            items = Array((items ?? []).map { clean($0, limit: 160) }.filter { !$0.isEmpty }.prefix(8))
            return items?.isEmpty == false
        case .note:
            return label?.isEmpty == false
        case .links:
            var seen = Set<URL>()
            links = Array((links ?? []).compactMap { link in
                guard let url = webURL(link.url), addresses.contains(url), seen.insert(url).inserted else {
                    return nil
                }
                return WidgetLink(label: clean(link.label, limit: 64), url: url.absoluteString)
            }.prefix(8))
            return links?.isEmpty == false
        case .metric:
            return label?.isEmpty == false && text?.isEmpty == false
        case .counter:
            value = min(max(value ?? 0, -100_000), 100_000)
            if let goal, !(1...100_000).contains(goal) { self.goal = nil }
            step = min(max(step ?? 1, 1), 100_000)
            return label?.isEmpty == false
        case .timer:
            guard let value else { return false }
            return (1...1440).contains(value)
        case .date:
            return label?.isEmpty == false && text?.count == 10
        case .clock:
            // Heion can resolve common city labels against its IANA database
            // when the model leaves the zone blank or supplies a city name.
            return true
        }
    }
}

@Generable
private struct WidgetDraft: Codable {
    @Guide(description: "A concise title in the user's language.")
    var title: String

    @Guide(description: "Return auto unless the user explicitly requested a size. Always use auto for an image unless the user specifies a size.")
    var size: WidgetSize

    @Guide(description: "One component for every part the user asks for, up to ten. Keep their order and include each distinct control. Add nothing the user did not request.")
    var blocks: [WidgetComponent]

    mutating func normalize(prompt: String) -> Bool {
        title = clean(title, limit: 64)
        let punctuation = CharacterSet(charactersIn: ".,()[]<>\"'〈〉")
        let addresses = Set(prompt.split(whereSeparator: \.isWhitespace).compactMap {
            webURL(String($0).trimmingCharacters(in: punctuation))
        })
        blocks = blocks.prefix(10).compactMap { original in
            var block = original
            return block.normalize(addresses: addresses) ? block : nil
        }
        return !title.isEmpty && !blocks.isEmpty
    }
}

private func clean(_ text: String, limit: Int) -> String {
    String(text.split(whereSeparator: \.isWhitespace).joined(separator: " ").prefix(limit))
}

private func webURL(_ address: String) -> URL? {
    guard address.utf8.count <= 2048, var parts = URLComponents(string: address),
          let scheme = parts.scheme?.lowercased(), ["https", "http"].contains(scheme),
          let host = parts.host, !host.isEmpty, parts.user == nil, parts.password == nil
    else { return nil }
    parts.scheme = scheme
    parts.host = host.lowercased()
    if parts.path.isEmpty { parts.path = "/" }
    return parts.url
}

private func send(_ data: Data, to callback: Completion, context: UnsafeMutableRawPointer?) {
    data.withUnsafeBytes { callback(context, $0.bindMemory(to: UInt8.self).baseAddress, $0.count) }
}

@MainActor private var generation: WidgetGeneration?

@MainActor
private final class WidgetGeneration {
    let context: UnsafeMutableRawPointer?
    let completion: Completion
    var task: Task<Void, Never>?

    init(context: UnsafeMutableRawPointer?, completion: @escaping Completion) {
        self.context = context
        self.completion = completion
    }

    func start(_ prompt: String) {
        task = Task { @MainActor [weak self] in
            var calendar = Calendar(identifier: .gregorian)
            calendar.timeZone = .current
            let day = calendar.dateComponents([.year, .month, .day], from: .now)
            let today = String(format: "%04d-%02d-%02d", day.year ?? 0, day.month ?? 0, day.day ?? 0)
            let session = LanguageModelSession(
                model: SystemLanguageModel.default,
                instructions: """
                    Create a real New Tab widget by composing the available native components.
                    Include every explicitly requested control, in the requested order. For example,
                    "a checklist, a 25 minute focus timer and a note" needs checklist, timer, note.
                    "A Lake Bled photo with editable Notes" needs image and note. Leave notes blank
                    unless initial text is requested.
                    Today's date is \(today), for interpreting relative dates only. Add a date
                    component only if the user asks for a date or event countdown.
                    Follow the user's subject, words, language, counts and sizes. Use text or
                    bullets for informative content, note for editable personal text, checklist
                    for tappable tasks, counter with an optional goal for trackers, timer for
                    a duration, date for an event countdown, clock for current time, metric
                    for fixed values, and links for explicit user-supplied addresses.
                    One clock component per requested city. Set label to the city and text to
                    its IANA time zone. Paris uses Europe/Paris, Kyiv uses Europe/Kyiv, and NYC
                    uses America/New_York. Do not reuse the system time zone for a named city.
                    For a photo or picture request, use an image component with a concise search
                    subject in text, or copy an explicit HTTPS image URL. For a cat picture use
                    kind image and text "domestic cat". Heion fetches and displays the actual image.
                    Write a short descriptive image label. Image requests need an image component.
                    Combine multiple components when asked. Do not invent current weather,
                    market quotes, calendar entries, private information or other live data.
                    If a requested capability needs an external service, explain that in a text
                    component and build the parts that can work locally.
                    Only add links if asked and full URLs were supplied. Avoid filler, such as
                    saying there are no links on widgets that do not request links.
                    """
            )
            do {
                var draft = try await session.respond(to: prompt, generating: WidgetDraft.self).content
                try Task.checkCancellation()
                guard draft.normalize(prompt: prompt) else {
                    self?.finish(nil, status: -1)
                    return
                }
                self?.finish(try JSONEncoder().encode(draft), status: -1)
            } catch {
                self?.finish(nil, status: Task.isCancelled ? 0 : -1)
            }
        }
    }

    func finish(_ data: Data?, status: Int) {
        guard generation === self else { return }
        generation = nil
        task = nil
        if let data { send(data, to: completion, context: context) }
        else { completion(context, nil, status) }
    }
}

@MainActor
@_cdecl("heion_widget_playground_available")
public func heionWidgetPlaygroundAvailable() -> Bool {
    SystemLanguageModel.default.isAvailable
}

@MainActor
@_cdecl("heion_widget_playground_generate")
public func heionWidgetPlaygroundGenerate(
    _ bytes: UnsafePointer<UInt8>,
    _ length: Int,
    _ context: UnsafeMutableRawPointer?,
    _ completion: @escaping Completion
) -> Bool {
    guard generation == nil, heionWidgetPlaygroundAvailable(), (1...8192).contains(length),
          let prompt = String(bytes: UnsafeBufferPointer(start: bytes, count: length), encoding: .utf8)
    else { return false }
    let request = WidgetGeneration(context: context, completion: completion)
    generation = request
    request.start(prompt)
    return true
}

@MainActor
@_cdecl("heion_widget_playground_cancel")
public func heionWidgetPlaygroundCancel() {
    guard let request = generation else { return }
    request.task?.cancel()
    request.finish(nil, status: 0)
}

@MainActor private var dictation: WidgetDictation?

@MainActor
private final class WidgetDictation {
    let context: UnsafeMutableRawPointer?
    let update: Completion
    let completion: Completion

    private var microphone: AVAudioEngine?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var recognizer: SFSpeechRecognizer?
    private var recognition: SFSpeechRecognitionTask?
    private var timeout: Task<Void, Never>?
    private var transcript = ""

    init(context: UnsafeMutableRawPointer?, update: @escaping Completion, completion: @escaping Completion) {
        self.context = context
        self.update = update
        self.completion = completion
    }

    func start() {
        Task { @MainActor [weak self] in
            guard let self else { return }
            guard let recognizer = SFSpeechRecognizer(),
                  recognizer.supportsOnDeviceRecognition else {
                finish(-4)
                return
            }
            let speechStatus = await withCheckedContinuation { continuation in
                SFSpeechRecognizer.requestAuthorization { continuation.resume(returning: $0) }
            }
            guard dictation === self else { return }
            guard speechStatus == .authorized else { finish(-3); return }
            let canRecord = await AVCaptureDevice.requestAccess(for: .audio)
            guard dictation === self else { return }
            guard canRecord else { finish(-2); return }
            guard recognizer.isAvailable else { finish(-4); return }
            record(using: recognizer)
        }
    }

    private func record(using recognizer: SFSpeechRecognizer) {
        let engine = AVAudioEngine()
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { finish(-1); return }
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.requiresOnDeviceRecognition = true
        request.shouldReportPartialResults = true
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { @Sendable buffer, _ in
            request.append(buffer)
        }
        self.request = request
        self.microphone = engine
        self.recognizer = recognizer
        recognition = recognizer.recognitionTask(with: request) { [weak self] result, error in
            let text = result?.bestTranscription.formattedString
            let ended = result?.isFinal == true || error != nil
            Task { @MainActor [weak self] in
                guard let self, dictation === self else { return }
                if let text, !text.isEmpty {
                    transcript = String(text.prefix(2000))
                    send(Data(transcript.utf8), to: update, context: context)
                }
                if ended { finish(transcript.isEmpty ? -5 : 0) }
            }
        }
        do {
            engine.prepare()
            try engine.start()
            update(context, nil, 0)
            timeout = Task { @MainActor [weak self] in
                try? await Task.sleep(for: .seconds(45))
                if !Task.isCancelled { self?.stop() }
            }
        } catch {
            finish(-1)
        }
    }

    private func releaseMicrophone() {
        if let microphone {
            microphone.stop()
            microphone.inputNode.removeTap(onBus: 0)
            self.microphone = nil
        }
    }

    func stop() {
        guard microphone != nil else { return }
        releaseMicrophone()
        request?.endAudio()
        request = nil
        recognition?.finish()
        timeout?.cancel()
        timeout = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(2))
            if !Task.isCancelled, let self { finish(transcript.isEmpty ? -5 : 0) }
        }
    }

    func finish(_ status: Int, discard: Bool = false) {
        guard dictation === self else { return }
        dictation = nil
        timeout?.cancel()
        timeout = nil
        releaseMicrophone()
        request?.endAudio()
        recognition?.cancel()
        recognition = nil
        request = nil
        recognizer = nil
        if status == 0, !discard, !transcript.isEmpty {
            send(Data(transcript.utf8), to: completion, context: context)
        } else {
            completion(context, nil, status)
        }
    }
}

@MainActor
@_cdecl("heion_widget_speech_start")
public func heionWidgetSpeechStart(
    _ context: UnsafeMutableRawPointer?,
    _ update: @escaping Completion,
    _ completion: @escaping Completion
) -> Bool {
    guard dictation == nil else { return false }
    let capture = WidgetDictation(context: context, update: update, completion: completion)
    dictation = capture
    capture.start()
    return true
}

@MainActor
@_cdecl("heion_widget_speech_stop")
public func heionWidgetSpeechStop() {
    dictation?.stop()
}

@MainActor
@_cdecl("heion_widget_speech_cancel")
public func heionWidgetSpeechCancel() {
    dictation?.finish(0, discard: true)
}
