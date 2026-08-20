export type Peer = { id: string; name: string; address: string; port: number; trusted: boolean };
export type IncomingOffer = {
  id: string;
  senderId: string;
  senderName: string;
  files: { name: string; size: number }[];
  text?: { preview: string; size: number };
};
export type AppSnapshot = {
  deviceId: string;
  deviceName: string;
  inbox: string;
  peers: Peer[];
  incoming: IncomingOffer[];
};
