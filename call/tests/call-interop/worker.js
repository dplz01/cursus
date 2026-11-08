// Frame encryption exactly as callCryptoFunction() in the official call.ts
// (audio frames: 1 plaintext byte, AES-GCM, 12-byte IV appended).
const IV_LENGTH = 12
const initialPlainTextRequired = {key: 10, delta: 3, empty: 1}

function decodeBase64url(s) {
  s = s.replace(/-/g, "+").replace(/_/g, "/").replace(/=+$/, "")
  const bin = atob(s + "===".slice((s.length + 3) % 4))
  return Uint8Array.from(bin, (c) => c.charCodeAt(0))
}

function concatN(...bs) {
  const a = new Uint8Array(bs.reduce((n, b) => n + b.byteLength, 0))
  bs.reduce((o, b) => (a.set(b, o), o + b.byteLength), 0)
  return a
}

function encryptFrame(key) {
  return async (frame, controller) => {
    const data = new Uint8Array(frame.data)
    const n = initialPlainTextRequired[frame.type] || 1
    const iv = crypto.getRandomValues(new Uint8Array(IV_LENGTH))
    const initial = data.subarray(0, n)
    const plaintext = data.subarray(n, data.byteLength)
    const ciphertext = plaintext.length
      ? new Uint8Array(await crypto.subtle.encrypt({name: "AES-GCM", iv: iv.buffer}, key, plaintext))
      : new Uint8Array(0)
    frame.data = concatN(initial, ciphertext, iv).buffer
    controller.enqueue(frame)
  }
}

function decryptFrame(key) {
  return async (frame, controller) => {
    const data = new Uint8Array(frame.data)
    const n = initialPlainTextRequired[frame.type] || 1
    const initial = data.subarray(0, n)
    const ciphertext = data.subarray(n, data.byteLength - IV_LENGTH)
    const iv = data.subarray(data.byteLength - IV_LENGTH, data.byteLength)
    try {
      const plaintext = ciphertext.length
        ? new Uint8Array(await crypto.subtle.decrypt({name: "AES-GCM", iv}, key, ciphertext))
        : new Uint8Array(0)
      frame.data = concatN(initial, plaintext).buffer
      controller.enqueue(frame)
    } catch (e) {
      // dropped, like a failed transform in call.ts
    }
  }
}

onrtctransform = async (event) => {
  const {readable, writable, options} = event.transformer
  const key = await crypto.subtle.importKey("raw", decodeBase64url(options.aesKey), {name: "AES-GCM", length: 256}, true, ["encrypt", "decrypt"])
  const transform = options.operation === "encrypt" ? encryptFrame(key) : decryptFrame(key)
  readable.pipeThrough(new TransformStream({transform})).pipeTo(writable)
}
