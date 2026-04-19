export function createStore(initial) {
  let state = initial;
  let listeners = [];

  // Pair 1: structurally identical getters
  // Both normalize to: function() { return $0 }
  function getCount() {
    return state;
  }

  function getLabel() {
    return state;
  }

  // Pair 2: structurally identical setters
  // Both normalize to: function($0) { $1 = $0 }
  function setCount(value) {
    state = value;
  }

  function setLabel(value) {
    state = value;
  }

  // Complex caller for getCount: loop + branch
  function processAll(items) {
    for (let i = 0; i < items.length; i++) {
      if (items[i] > 0) {
        console.log(getCount());
      }
    }
  }

  // Linear caller for getLabel
  function display() {
    return String(getLabel());
  }

  // Branching caller for setCount
  function updateFromInput(input) {
    if (typeof input === "number") {
      setCount(input);
    } else if (typeof input === "string") {
      setCount(parseInt(input));
    }
  }

  // Linear caller for setLabel
  function initialize(config) {
    setLabel(config.label);
  }

  function subscribe(listener) {
    listeners.push(listener);
  }

  function unsubscribe(listener) {
    let idx = listeners.indexOf(listener);
    if (idx >= 0) {
      listeners.splice(idx, 1);
    }
  }

  function notify() {
    for (let i = 0; i < listeners.length; i++) {
      listeners[i](state);
    }
  }

  function reset() {
    state = initial;
  }

  return {
    getCount: getCount,
    getLabel: getLabel,
    setCount: setCount,
    setLabel: setLabel,
    processAll: processAll,
    display: display,
    updateFromInput: updateFromInput,
    initialize: initialize,
    subscribe: subscribe,
    unsubscribe: unsubscribe,
    notify: notify,
    reset: reset
  };
}
