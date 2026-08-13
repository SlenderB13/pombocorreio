import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import { Bird, Check, File, FileUp, RefreshCw, Send, Smartphone, X } from "lucide-react";
import type { AppSnapshot, IncomingOffer, Peer } from "./types";
import "./App.css";

const emptySnapshot: AppSnapshot = { deviceId: "", deviceName: "This device", inbox: "", peers: [], incoming: [] };
const basename = (path: string) => path.split(/[\\/]/).pop() ?? path;
type SelectedFile = { path: string; name: string };

function displayName(path: string) {
  const tail = basename(path);
  try {
    const decoded = decodeURIComponent(tail);
    return decoded.split(":").pop()?.split("/").pop() || "Selected file";
  } catch { return tail || "Selected file"; }
}

function App() {
  const [snapshot, setSnapshot] = useState(emptySnapshot);
  const [files, setFiles] = useState<SelectedFile[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [dragging, setDragging] = useState(false);
  const [status, setStatus] = useState("Looking for nearby devices…");
  const refresh = async () => setSnapshot(await invoke<AppSnapshot>("snapshot"));

  useEffect(() => {
    refresh().catch((error) => setStatus(String(error)));
    const cleanup = Promise.all([
      listen("state-changed", refresh),
      getCurrentWebviewWindow().onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "over") setDragging(true);
        if (payload.type === "leave") setDragging(false);
        if (payload.type === "drop") {
          setDragging(false);
          setFiles((current) => {
            const next = payload.paths.map((path) => ({ path, name: basename(path) }));
            return [...new Map([...current, ...next].map((file) => [file.path, file])).values()];
          });
        }
      }),
    ]);
    return () => void cleanup.then((callbacks) => callbacks.forEach((fn) => fn()));
  }, []);

  const selectedPeers = useMemo(() => snapshot.peers.filter((peer) => selected.includes(peer.id)), [snapshot.peers, selected]);

  async function send() {
    if (!files.length || !selectedPeers.length) return;
    setStatus("Waiting for the receiving device…");
    try {
      await invoke("send_files", { peerIds: selected, files });
      setFiles([]);
      setStatus("Sent successfully");
    } catch (error) { setStatus(String(error)); }
  }

  async function pickFiles() {
    try {
      const picked = await open({ multiple: true, directory: false, pickerMode: "document", fileAccessMode: "copy" });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      setFiles((current) => {
        const next = paths.map((path) => ({ path, name: displayName(path) }));
        return [...new Map([...current, ...next].map((file) => [file.path, file])).values()];
      });
      setStatus(`${paths.length} file${paths.length === 1 ? "" : "s"} selected`);
    } catch (error) { setStatus(`Could not open file picker: ${error}`); }
  }

  async function answer(offer: IncomingOffer, accept: boolean) {
    await invoke("answer_offer", { offerId: offer.id, accept, trust: accept });
    await refresh();
  }

  return <main>
    <header><div className="brand"><Bird size={22} /></div><div className="identity"><h1>Pombo Correio</h1><p>{snapshot.deviceName}</p></div><div className="online"><i /> Online</div></header>
    {snapshot.incoming.map((offer) => <section className="incoming" key={offer.id}>
      <div><strong>{offer.senderName}</strong> wants to send {offer.files.length} file{offer.files.length === 1 ? "" : "s"}.</div>
      <small>{offer.files.map((file) => file.name).join(", ")}</small>
      <div className="actions"><button className="quiet" onClick={() => answer(offer, false)}><X size={16}/> Decline</button><button onClick={() => answer(offer, true)}><Check size={16}/> Accept & trust</button></div>
    </section>)}
    <section className={`dropzone ${dragging ? "dragging" : ""}`} role="button" tabIndex={0} onClick={pickFiles} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") void pickFiles(); }}>
      <div className="arrow"><FileUp size={24}/></div><h2>{files.length ? `${files.length} file${files.length === 1 ? "" : "s"} ready` : "Choose files"}</h2>
      <p>{files.length ? "Tap to add more" : "Tap to browse or drop files here"}</p>
      {files.length > 0 && <div className="file-list">{files.slice(0, 3).map((file) => <span className="file-pill" key={file.path}><File size={13}/>{file.name}</span>)}{files.length > 3 && <span className="file-pill">+{files.length - 3} more</span>}</div>}
      {files.length > 0 && <button className="link clear" onClick={(event) => { event.stopPropagation(); setFiles([]); }}><X size={14}/> Clear</button>}
    </section>
    <section className="devices"><div className="section-title"><h2>Nearby devices</h2><button className="link" onClick={refresh}><RefreshCw size={14}/> Refresh</button></div>
      {snapshot.peers.length === 0 ? <div className="empty">Open Pombo Correio on another device connected to this network.</div> : snapshot.peers.map((peer: Peer) => <label className={`device ${selected.includes(peer.id) ? "selected" : ""}`} key={peer.id}>
        <input type="checkbox" checked={selected.includes(peer.id)} onChange={() => setSelected((items) => items.includes(peer.id) ? items.filter((id) => id !== peer.id) : [...items, peer.id])} />
        <span className="device-icon"><Smartphone size={18}/></span><span><strong>{peer.name}</strong><small>{peer.trusted ? "Trusted" : "First transfer needs approval"}</small></span>
      </label>)}
    </section>
    <footer><span className="status" title={snapshot.inbox}><i />{status}</span><button className="send-button" disabled={!files.length || !selected.length} onClick={send}><Send size={16}/> Send{selected.length ? ` to ${selected.length}` : ""}</button></footer>
  </main>;
}

export default App;
