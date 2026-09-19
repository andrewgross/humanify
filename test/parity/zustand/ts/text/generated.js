const createStoreImplementation = initialStateOrFactory => {
  let currentState;
  const subscribers = new Set();
  const setState = (newStateOrUpdater, replaceFlag) => {
    const newState = typeof newStateOrUpdater == "function" ? newStateOrUpdater(currentState) : newStateOrUpdater;
    if (!Object.is(newState, currentState)) {
      const previousState = currentState;
      if (replaceFlag ?? (typeof newState != "object" || newState === null)) {
        currentState = newState;
      } else {
        currentState = Object.assign({}, currentState, newState);
      }
      subscribers.forEach(handleStateTransition => handleStateTransition(currentState, previousState));
    }
  };
  const getState = () => currentState;
  const store = {
    setState: setState,
    getState: getState,
    getInitialState: () => initialState,
    subscribe: subscriber => (subscribers.add(subscriber), () => subscribers.delete(subscriber))
  };
  const initialState = currentState = initialStateOrFactory(setState, getState, store);
  return store;
};
const createZustandStore = initialStateOrFactory => initialStateOrFactory ? createStoreImplementation(initialStateOrFactory) : createStoreImplementation;
export { createZustandStore as createStore };