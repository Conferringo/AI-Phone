import Foundation
import Vision
import ImageIO
import CoreGraphics

struct Box: Codable { let left: Int; let top: Int; let right: Int; let bottom: Int }
struct TextRegion: Codable { let text: String; let confidence: Float; let bounds: Box }
struct Capture: Codable { let width: Int; let height: Int; let regions: [TextRegion]; let grid: [UInt8] }
do {
    let data = FileHandle.standardInput.readDataToEndOfFile()
    guard data.count <= 2_097_152,
          let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
          image.width > 0, image.height > 0, image.width <= 4096, image.height <= 4096
    else { throw NSError(domain: "phone-vision", code: 1) }
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = false
    let supported = try request.supportedRecognitionLanguages()
    request.recognitionLanguages = ["zh-Hans", "en-US"].filter { supported.contains($0) }
    try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
    let w = image.width, h = image.height
    let regions = (request.results ?? []).prefix(200).compactMap { observation -> TextRegion? in
        guard let text = observation.topCandidates(1).first, text.confidence >= 0.70 else { return nil }
        let b = observation.boundingBox
        let box = Box(left: max(0, Int(floor(b.minX * Double(w)))),
                      top: max(0, Int(floor((1 - b.maxY) * Double(h)))),
                      right: min(w, Int(ceil(b.maxX * Double(w)))),
                      bottom: min(h, Int(ceil((1 - b.minY) * Double(h)))))
        guard box.right > box.left, box.bottom > box.top else { return nil }
        return TextRegion(text: String(text.string.prefix(400)), confidence: text.confidence, bounds: box)
    }
    var grid = [UInt8](repeating: 0, count: 32 * 64)
    let drawn = grid.withUnsafeMutableBytes { bytes -> Bool in
        guard let context = CGContext(data: bytes.baseAddress, width: 32, height: 64,
            bitsPerComponent: 8, bytesPerRow: 32, space: CGColorSpaceCreateDeviceGray(), bitmapInfo: 0) else { return false }
        context.interpolationQuality = .low
        context.draw(image, in: CGRect(x: 0, y: 0, width: 32, height: 64))
        return true
    }
    guard drawn else { throw NSError(domain: "phone-vision", code: 2) }
    FileHandle.standardOutput.write(try JSONEncoder().encode(Capture(width: w, height: h, regions: regions, grid: grid)))
} catch {
    FileHandle.standardError.write(Data("Phone OCR unavailable: \(error.localizedDescription)\n".utf8))
    exit(1)
}
