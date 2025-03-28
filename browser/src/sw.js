self.addEventListener('fetch', event => {
    // Clona la richiesta per poter leggere il corpo (se necessario) senza consumarla
    const reqClone = event.request.clone();
  
    // Funzione asincrona per creare il riepilogo della richiesta
    async function summarizeRequest(req) {
      // Estrai le intestazioni in un oggetto
      const headersObj = {};
      req.headers.forEach((value, key) => {
        headersObj[key] = value;
      });
  
      // Inizializza il corpo a null
      let body = null;
      // Leggi il corpo se il metodo prevede un payload (ad es. POST, PUT, ecc.)
      if (req.method !== 'GET' && req.method !== 'HEAD') {
        try {
          body = await req.text();
        } catch (error) {
          console.error("Errore nella lettura del corpo:", error);
        }
      }
  
      // Crea l'oggetto JSON di riepilogo
      const summary = {
        method: req.method,
        url: req.url,
        headers: headersObj,
        body: body
      };
  
      return summary;
    }
  
    // Usa respondWith per continuare la catena della fetch originale
    event.respondWith(
      (async () => {
        // Crea il riepilogo della richiesta
        const requestSummary = await summarizeRequest(reqClone);
        // Logga l'oggetto JSON nel console del Service Worker
        console.log("Riepilogo della richiesta:", requestSummary);
  
        // Invia la richiesta originale
        return fetch(event.request);
      })()
    );
  });