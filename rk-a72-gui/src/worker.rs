use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use rk_a72_keymap::KeyMatrixRepository;

/// Commands the UI thread sends to the HID worker thread. `WriteLayer` carries the
/// FULL patched buffer — the UI thread owns the in-memory layer buffers and does the
/// slot-level patching itself (there is no slot-level write opcode on the device;
/// `KeyMatrixRepository::write_layer` always writes a whole layer).
#[derive(Debug, Clone, PartialEq)]
pub enum GuiCommand {
    ReadLayer(u8),
    WriteLayer { layer: u8, buffer: Vec<u8> },
}

/// Events the worker thread sends back to the UI thread. `DeviceLost` means the
/// worker's HID session failed and the thread is about to exit — the UI must treat
/// the device as disconnected and offer to reconnect.
#[derive(Debug, Clone, PartialEq)]
pub enum GuiEvent {
    LayerLoaded { layer: u8, buffer: Vec<u8> },
    LayerWritten { layer: u8, ok: bool },
    DeviceLost,
}

/// Spawns the dedicated HID worker thread. It owns `repo` for its entire lifetime,
/// executing one `GuiCommand` at a time and reporting results via `event_tx`. The
/// thread exits (dropping `repo`, closing the HID handle) on the first I/O error or
/// once `cmd_rx`'s sender is dropped.
pub fn spawn_worker(
    repo: KeyMatrixRepository,
    cmd_rx: Receiver<GuiCommand>,
    event_tx: Sender<GuiEvent>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                GuiCommand::ReadLayer(layer) => match repo.read_layer(layer) {
                    Ok(buffer) => {
                        if event_tx
                            .send(GuiEvent::LayerLoaded { layer, buffer })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = event_tx.send(GuiEvent::DeviceLost);
                        return;
                    }
                },
                GuiCommand::WriteLayer { layer, buffer } => {
                    let ok = repo.write_layer(layer, &buffer).is_ok();
                    if event_tx.send(GuiEvent::LayerWritten { layer, ok }).is_err() {
                        return;
                    }
                    if !ok {
                        let _ = event_tx.send(GuiEvent::DeviceLost);
                        return;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_and_events_are_constructible_and_comparable() {
        // No real HID device is available in a test environment, so this only
        // exercises the plain-data parts of the module (spawn_worker itself is
        // covered by the manual real-hardware check in the final task).
        let a = GuiCommand::ReadLayer(0);
        let b = GuiCommand::ReadLayer(0);
        assert_eq!(a, b);

        let c = GuiCommand::WriteLayer {
            layer: 1,
            buffer: vec![0u8; 4],
        };
        assert_ne!(a, c);

        let e1 = GuiEvent::LayerLoaded {
            layer: 0,
            buffer: vec![1, 2, 3],
        };
        let e2 = GuiEvent::LayerLoaded {
            layer: 0,
            buffer: vec![1, 2, 3],
        };
        assert_eq!(e1, e2);
        assert_ne!(e1, GuiEvent::DeviceLost);
    }
}
