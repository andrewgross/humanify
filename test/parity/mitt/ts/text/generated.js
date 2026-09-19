export default function createEventEmitter(eventMap) {
  return {
    all: eventMap = eventMap || new Map(),
    on(eventType, listener) {
      const listeners = eventMap.get(eventType);
      if (listeners) {
        listeners.push(listener);
      } else {
        eventMap.set(eventType, [listener]);
      }
    },
    off(eventName, listener) {
      const listeners = eventMap.get(eventName);
      if (listeners) {
        if (listener) {
          listeners.splice(listeners.indexOf(listener) >>> 0, 1);
        } else {
          eventMap.set(eventName, []);
        }
      }
    },
    emit(eventName, eventData) {
      let handlers = eventMap.get(eventName);
      if (handlers) {
        handlers.slice().map(eventHandler => {
          eventHandler(eventData);
        });
      }
      handlers = eventMap.get("*");
      if (handlers) {
        handlers.slice().map(eventHandler => {
          eventHandler(eventName, eventData);
        });
      }
    }
  };
}