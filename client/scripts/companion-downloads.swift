import AppKit
import Foundation

@main
struct CompanionDownloads {
    @MainActor static func main() {
        guard CommandLine.arguments.count == 3, ["reranker", "clef"].contains(CommandLine.arguments[1]) else { exit(2) }
        let kind = CommandLine.arguments[1]
        let runtime = URL(fileURLWithPath: CommandLine.arguments[2])
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let reranker = RerankerDownload(), clef = ClefFlashDownload()
        reranker.preparationEngine = runtime.appendingPathComponent("PhotoEngine")
        var started = false
        var last = Data()
        func emit() {
            let running = kind == "reranker" ? reranker.running : clef.running
            var value: [String: Any] = ["kind": kind, "running": running,
                "progress": kind == "reranker" ? reranker.progress : clef.progress,
                "message": kind == "reranker" ? reranker.message : clef.message]
            if kind == "reranker", let path = reranker.downloadedPath { value["rerankerPath"] = path }
            if kind == "clef", let files = clef.downloadedFiles { value["modelPath"] = files.model; value["projectorPath"] = files.projector }
            if let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]), data != last {
                FileHandle.standardOutput.write(data); FileHandle.standardOutput.write(Data([10])); last = data
            }
            if started && !running { exit(0) }
        }
        DispatchQueue.global().async {
            while let line = readLine() {
                if line == "cancel" {
                    DispatchQueue.main.async {
                        if kind == "reranker" { reranker.cancel() } else { clef.cancel() }
                    }
                }
            }
            DispatchQueue.main.async { if kind == "reranker" { reranker.cancel() } else { clef.cancel() }; exit(0) }
        }
        _ = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { _ in MainActor.assumeIsolated { emit() } }
        DispatchQueue.main.async {
            app.activate(ignoringOtherApps: true)
            if kind == "reranker" { reranker.chooseDestinationAndStart() } else { clef.chooseDestinationAndStart() }
            started = true
            emit()
        }
        app.run()
    }
}
