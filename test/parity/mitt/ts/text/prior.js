export default function createEventEmitter(eventMap) {
  return {
    all: eventMap = eventMap || new Map(),
    on(eventType, listener) {
      const listeners = eventMap.get(eventType);
      if (!(listeners && listeners.push(listener))) {
        eventMap.set(eventType, [listener]);
      }
    },
    off(eventName, listener) {
      const listeners = eventMap.get(eventName);
      if (listeners) {
        listeners.splice(listeners.indexOf(listener) >>> 0, 1);
      }
    },
    emit(eventName, eventData) {
      (eventMap.get(eventName) || []).slice().map(eventHandler => {
        eventHandler(eventData);
      });
      (eventMap.get("*") || []).slice().map(eventHandler => {
        eventHandler(eventName, eventData);
      });
    }
  };
}