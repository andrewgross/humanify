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
    subscribe: subscriber => (subscribers.add(subscriber), () => subscribers.delete(subscriber)),
    destroy: () => {
      if (import.meta.env?.MODE !== "production") {
        console.warn("[DEPRECATED] The `destroy` method will be unsupported in a future version. Instead use unsubscribe function returned by subscribe. Everything will be garbage-collected if store is garbage-collected.");
      }
      subscribers.clear();
    }
  };
  const initialState = currentState = initialStateOrFactory(setState, getState, store);
  return store;
};
const createZustandStore = initialStateOrFactory => initialStateOrFactory ? createStoreImplementation(initialStateOrFactory) : createStoreImplementation;
export { createZustandStore as createStore };
export default initialStateOrFactory => (import.meta.env?.MODE !== "production" && console.warn("[DEPRECATED] Default export is deprecated. Instead use import { createStore } from 'zustand/vanilla'."), createZustandStore(initialStateOrFactory));