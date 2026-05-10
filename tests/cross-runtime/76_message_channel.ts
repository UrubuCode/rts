// Cross-runtime compatibility: MessageChannel delivery.
const channel = new MessageChannel();
channel.port1.onmessage = (ev) => {
  console.log("recv=" + ev.data);
  channel.port1.close();
  channel.port2.close();
};
channel.port2.postMessage("pong");
