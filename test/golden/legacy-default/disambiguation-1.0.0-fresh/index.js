export function createStore(initialRenamed) {
  let stateRenamed = initialRenamed;
  let listenersRenamed = [];
  function getCountRenamed() {
    return stateRenamed;
  }
  function getLabelRenamed() {
    return stateRenamed;
  }
  function setCountRenamed(valueRenamed) {
    stateRenamed = valueRenamed;
  }
  function setLabelRenamed(valueRenamed) {
    stateRenamed = valueRenamed;
  }
  function processAllRenamed(itemsRenamed) {
    for (let iRenamed = 0; iRenamed < itemsRenamed.length; iRenamed++) {
      if (itemsRenamed[iRenamed] > 0) {
        console.log(getCountRenamed());
      }
    }
  }
  function displayRenamed() {
    return String(getLabelRenamed());
  }
  function updateFromInputRenamed(inputRenamed) {
    if (typeof inputRenamed === "number") {
      setCountRenamed(inputRenamed);
    } else if (typeof inputRenamed === "string") {
      setCountRenamed(parseInt(inputRenamed));
    }
  }
  function initializeRenamed(configRenamed) {
    setLabelRenamed(configRenamed.label);
  }
  function subscribeRenamed(listenerRenamed) {
    listenersRenamed.push(listenerRenamed);
  }
  function unsubscribeRenamed(listenerRenamed) {
    let idxRenamed = listenersRenamed.indexOf(listenerRenamed);
    if (idxRenamed >= 0) {
      listenersRenamed.splice(idxRenamed, 1);
    }
  }
  function notifyRenamed() {
    for (let iRenamed = 0; iRenamed < listenersRenamed.length; iRenamed++) {
      listenersRenamed[iRenamed](stateRenamed);
    }
  }
  function resetRenamed() {
    stateRenamed = initialRenamed;
  }
  return {
    getCount: getCountRenamed,
    getLabel: getLabelRenamed,
    setCount: setCountRenamed,
    setLabel: setLabelRenamed,
    processAll: processAllRenamed,
    display: displayRenamed,
    updateFromInput: updateFromInputRenamed,
    initialize: initializeRenamed,
    subscribe: subscribeRenamed,
    unsubscribe: unsubscribeRenamed,
    notify: notifyRenamed,
    reset: resetRenamed
  };
}