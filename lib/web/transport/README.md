# NubilaTransport

NubilaTransport is a transport for bare-mux which allows you to use [nubila](https://TODO) in your bare client.

Usage of this transport is as simple as setting the transport and providing a key hash. An example is shown below where you get connected to the public Nubila server.

```js
import { SetTransport } from '@mercuryworkshop/bare-mux';

SetTransport("EpxMod.NubilaClient", { hash: "TODO" });
```