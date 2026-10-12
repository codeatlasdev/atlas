// winrec <windowID> <seconds> <out.mov> [fps]
//
// Records ONE window (by CGWindowID) with ScreenCaptureKit into an HEVC .mov at 2x the window size.
// It never sends input and never captures anything but that window. Exit codes:
//   0 ok · 1 recording failed · 2 bad usage · 3 Screen Recording permission missing · 4 window not found
import Foundation
import ScreenCaptureKit
import AVFoundation
import CoreMedia
import AppKit

_ = NSApplication.shared
setvbuf(stdout, nil, _IOLBF, 0)

func die(_ code: Int32, _ message: String) -> Never {
	FileHandle.standardError.write(Data("winrec: \(message)\n".utf8))
	exit(code)
}

let args = CommandLine.arguments
guard args.count >= 4, let wid = UInt32(args[1]), let secs = Double(args[2]), secs > 0 else {
	die(2, "usage: winrec <windowID> <seconds> <out.mov> [fps]")
}
let out = URL(fileURLWithPath: args[3])
let fps = args.count > 4 ? Int(args[4]) ?? 60 : 60

final class Rec: NSObject, SCStreamOutput, SCStreamDelegate {
	var writer: AVAssetWriter!
	var input: AVAssetWriterInput!
	var started = false
	var frames = 0

	func stream(_ s: SCStream, didOutputSampleBuffer sb: CMSampleBuffer, of type: SCStreamOutputType) {
		guard type == .screen, sb.isValid,
			let att = CMSampleBufferGetSampleAttachmentsArray(sb, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
			let status = att.first?[.status] as? Int, status == SCFrameStatus.complete.rawValue else { return }
		if !started {
			writer.startWriting()
			writer.startSession(atSourceTime: CMSampleBufferGetPresentationTimeStamp(sb))
			started = true
		}
		if input.isReadyForMoreMediaData { input.append(sb); frames += 1 }
	}
}

Task {
	do {
		let content: SCShareableContent
		do {
			content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
		} catch {
			die(3, "cannot list windows (\(error.localizedDescription)). Grant Screen Recording to the app running this command: System Settings > Privacy & Security > Screen Recording")
		}
		guard let win = content.windows.first(where: { $0.windowID == wid }) else {
			die(4, "window \(wid) not found (closed, minimized to another space, or Screen Recording is not granted)")
		}
		let cfg = SCStreamConfiguration()
		let scale = 2
		cfg.width = Int(win.frame.width) * scale
		cfg.height = Int(win.frame.height) * scale
		cfg.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(fps))
		cfg.showsCursor = false
		cfg.pixelFormat = kCVPixelFormatType_32BGRA
		cfg.queueDepth = 6

		let rec = Rec()
		try? FileManager.default.removeItem(at: out)
		rec.writer = try AVAssetWriter(outputURL: out, fileType: .mov)
		rec.input = AVAssetWriterInput(mediaType: .video, outputSettings: [
			AVVideoCodecKey: AVVideoCodecType.hevc,
			AVVideoWidthKey: cfg.width,
			AVVideoHeightKey: cfg.height,
			AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: 40_000_000],
		])
		rec.input.expectsMediaDataInRealTime = true
		rec.writer.add(rec.input)

		let stream = SCStream(filter: SCContentFilter(desktopIndependentWindow: win), configuration: cfg, delegate: rec)
		try stream.addStreamOutput(rec, type: .screen, sampleHandlerQueue: DispatchQueue(label: "winrec"))
		try await stream.startCapture()
		print("recording")  // the caller waits for this line before it starts the scene
		try await Task.sleep(nanoseconds: UInt64(secs * 1e9))
		try await stream.stopCapture()
		rec.input.markAsFinished()
		await rec.writer.finishWriting()
		if rec.frames == 0 { die(1, "no frames were captured (is the window visible and not occluded by a lock screen?)") }
		print("frames \(rec.frames) \(cfg.width)x\(cfg.height)")
	} catch {
		die(1, "\(error)")
	}
	exit(0)
}
RunLoop.main.run()
