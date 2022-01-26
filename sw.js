const cacheName = 'loqui-pwa';

/* Start the service worker and cache all of the app's content */
self.addEventListener("install", installEvent => {
    installEvent.waitUntil(
      caches.open(cacheName).then(cache => {
        cache.addAll(cacheFiles)
      })
    )
});

/* Serve cached content when offline */
self.addEventListener("fetch", fetchEvent => {
    fetchEvent.respondWith(
      caches.match(fetchEvent.request).then(res => {
        return res || fetch(fetchEvent.request)
      })
    )
});