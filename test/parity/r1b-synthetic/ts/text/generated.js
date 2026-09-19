export function createCounterStore(initialCount) {
  let currentCount = initialCount;
  let subscribers = [];
  function getResult() {
    return currentCount;
  }
  function getValue() {
    return currentCount;
  }
  function setValue(value) {
    currentCount = value;
  }
  function setLabelValue(labelValue) {
    currentCount = labelValue;
  }
  return {
    getCount: getResult,
    getLabel: getValue,
    setCount: setValue,
    setLabel: setLabelValue,
    processAll: function (items) {
      for (let index = 0; index < items.length; index++) {
        if (items[index] > 0) {
          console.log(getResult());
        }
      }
    },
    display: function () {
      return String(getValue());
    },
    updateFromInput: function (input) {
      if (typeof input == "number") {
        setValue(input);
      } else if (typeof input == "string") {
        setValue(parseInt(input));
      }
    },
    initialize: function (labelData) {
      setLabelValue(labelData.label);
    },
    subscribe: function (item) {
      subscribers.push(item);
    },
    unsubscribe: function (elementToRemove) {
      let index = subscribers.indexOf(elementToRemove);
      if (index >= 0) {
        subscribers.splice(index, 1);
      }
    },
    notify: function () {
      for (let index = 0; index < subscribers.length; index++) {
        subscribers[index](currentCount);
      }
    },
    reset: function () {
      let previousCount = currentCount;
      currentCount = initialCount;
      return previousCount;
    }
  };
}