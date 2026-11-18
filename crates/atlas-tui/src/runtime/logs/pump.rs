//! Plumbing from a child's pipe to log lines: one task per stream.

use std::time::Duration;

/// How long a partial line may sit before it is shown anyway (prompts and
/// progress output that never send a newline).
pub(super) const PARTIAL_FLUSH_AFTER: Duration = Duration::from_millis(150);

/// Read `reader` on its own task, handing each assembled line to `on_line`
/// (see `linebuf::LineAssembler`: partial lines, CRLF / `\r` progress bars,
/// lossy UTF-8 and the per-line cap are handled there).
/// The handle lets the owner abort the pump when the process it reads is gone.
pub(crate) fn spawn_line_pump<R>(
    mut reader: R,
    mut on_line: impl FnMut(String) + Send + 'static,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::AsyncReadExt;
    tokio::spawn(async move {
        let mut asm = crate::runtime::linebuf::LineAssembler::new();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            // Waiting on a partial line: give it a moment, then show it as is.
            let read = if asm.has_partial() {
                match tokio::time::timeout(PARTIAL_FLUSH_AFTER, reader.read(&mut buf)).await {
                    Ok(r) => r,
                    Err(_) => {
                        asm.flush(|l| on_line(l.text));
                        continue;
                    }
                }
            } else {
                reader.read(&mut buf).await
            };
            match read {
                Ok(0) | Err(_) => break,
                Ok(n) => asm.push(&buf[..n], |l| on_line(l.text)),
            }
        }
        // EOF: a final line without a newline still counts.
        asm.flush(|l| on_line(l.text));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pump_assembles_partial_lines_cr_and_flushes_on_idle_and_eof() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            use tokio::io::AsyncWriteExt;
            let (mut w, r) = tokio::io::duplex(1024);
            let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let sink = out.clone();
            let h = spawn_line_pump(r, move |l| sink.lock().unwrap().push(l));
            w.write_all(b"he").await.unwrap();
            w.write_all(b"llo\r\nprog 1%\rprog 99%\rdone\nprompt> ")
                .await
                .unwrap();
            // Idle: the partial prompt must show up without a newline, while the writer is still open. Wait
            // for that event (the idle flush), not for a multiple of its delay: a stalled runner can
            // outlast any fixed sleep. Without the flush it never comes and the hang ceiling fails.
            let end = tokio::time::Instant::now() + atlas_core::scaling::HANG;
            while out.lock().unwrap().len() < 3 && tokio::time::Instant::now() < end {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert_eq!(*out.lock().unwrap(), ["hello", "done", "prompt> "]);
            w.write_all(b"tail-no-newline").await.unwrap();
            drop(w);
            h.await.unwrap();
            assert_eq!(out.lock().unwrap().last().unwrap(), "tail-no-newline");
        });
    }
}
