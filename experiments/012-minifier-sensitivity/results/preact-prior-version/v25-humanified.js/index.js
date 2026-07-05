var HYDRATE_MODE = 32;
var SUSPENDED_MODE = 128;
var INSERT_VNODE_FLAG = 65536;
var MATCHED_FLAG = 1 << 17;
var RESET_FLAGS_MASK = ~(HYDRATE_MODE | SUSPENDED_MODE);
var UNDEFINED_VALUE = undefined;
var EMPTY_OBJECT = {};
var EMPTY_ARRAY = [];
var NON_DIMENSIONAL_REGEX =
  /acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i;
var isArrayFn = Array.isArray;
function mergeObjects(targetObject, sourceObject) {
  for (let nextState in sourceObject)
    targetObject[nextState] = sourceObject[nextState];
  return targetObject;
}
function removeDomNode(node) {
  if (node && node.parentNode) {
    node.parentNode.removeChild(node);
  }
}
var arraySlice = EMPTY_ARRAY.slice;
function handleError(error, currentComponent, unusedParam, errorInfo) {
  let componentInstance, componentConstructor, isDirty;
  for (; (currentComponent = currentComponent._parent); ) {
    if (
      (componentInstance = currentComponent._component) &&
      !componentInstance._processingException
    ) {
      try {
        componentConstructor = componentInstance.constructor;
        if (
          componentConstructor &&
          componentConstructor.getDerivedStateFromError != null
        ) {
          componentInstance.setState(
            componentConstructor.getDerivedStateFromError(error)
          );
          isDirty = componentInstance._dirty;
        }
        if (componentInstance.componentDidCatch != null) {
          componentInstance.componentDidCatch(error, errorInfo || {});
          isDirty = componentInstance._dirty;
        }
        if (isDirty) {
          return (componentInstance._pendingError = componentInstance);
        }
      } catch (innerError) {
        error = innerError;
      }
    }
  }
  throw error;
}
var catchErrorOptions = {
  _catchError: handleError
};
var defaultOptions = catchErrorOptions;
var vnodeIdCounter = 0;
function createVirtualElement(nodeType, props, children) {
  let key, ref, propName;
  let propsWithDefaults = {};
  for (propName in props)
    if (propName == "key") {
      key = props[propName];
    } else if (propName == "ref") {
      ref = props[propName];
    } else {
      propsWithDefaults[propName] = props[propName];
    }
  if (arguments.length > 2) {
    propsWithDefaults.children =
      arguments.length > 3 ? arraySlice.call(arguments, 2) : children;
  }
  if (typeof nodeType == "function" && nodeType.defaultProps != null) {
    for (propName in nodeType.defaultProps)
      if (propsWithDefaults[propName] === UNDEFINED_VALUE) {
        propsWithDefaults[propName] = nodeType.defaultProps[propName];
      }
  }
  return createVirtualNode(nodeType, propsWithDefaults, key, ref, null);
}
function createVirtualNode(nodeType, nodeProps, nodeKey, nodeRef, originalId) {
  const vnode = {
    type: nodeType,
    props: nodeProps,
    key: nodeKey,
    ref: nodeRef,
    _children: null,
    _parent: null,
    _depth: 0,
    _dom: null,
    _nextDom: UNDEFINED_VALUE,
    _component: null,
    constructor: UNDEFINED_VALUE,
    _original: originalId == null ? ++vnodeIdCounter : originalId,
    _index: -1,
    _flags: 0
  };
  if (originalId == null && defaultOptions.vnode != null) {
    defaultOptions.vnode(vnode);
  }
  return vnode;
}
function createRefObject() {
  return {
    current: null
  };
}
function getFragmentChildren(element) {
  return element.children;
}
var isValidReactElement = (value) =>
  value != null && value.constructor == UNDEFINED_VALUE;
function Component(props, context) {
  this.props = props;
  this.context = context;
}
function getNextDomSibling(vnode, childIndex) {
  if (childIndex == null) {
    if (vnode._parent) {
      return getNextDomSibling(vnode._parent, vnode._index + 1);
    } else {
      return null;
    }
  }
  let childNode;
  for (; childIndex < vnode._children.length; childIndex++) {
    childNode = vnode._children[childIndex];
    if (childNode != null && childNode._dom != null) {
      return childNode._dom;
    }
  }
  if (typeof vnode.type == "function") {
    return getNextDomSibling(vnode);
  } else {
    return null;
  }
}
function renderComponentInstance(componentInstance) {
  let vnode = componentInstance._vnode;
  let domNode = vnode._dom;
  let addedNodes = [];
  let removedNodes = [];
  if (componentInstance._parentDom) {
    const mergedVnode = mergeObjects({}, vnode);
    mergedVnode._original = vnode._original + 1;
    if (defaultOptions.vnode) {
      defaultOptions.vnode(mergedVnode);
    }
    diffVirtualNodes(
      componentInstance._parentDom,
      mergedVnode,
      vnode,
      componentInstance._globalContext,
      componentInstance._parentDom.namespaceURI,
      vnode._flags & HYDRATE_MODE ? [domNode] : null,
      addedNodes,
      domNode == null ? getNextDomSibling(vnode) : domNode,
      !!(vnode._flags & HYDRATE_MODE),
      removedNodes
    );
    mergedVnode._original = vnode._original;
    mergedVnode._parent._children[mergedVnode._index] = mergedVnode;
    commitRootComponent(addedNodes, mergedVnode, removedNodes);
    if (mergedVnode._dom != domNode) {
      propagateDomPointer(mergedVnode);
    }
  }
}
function propagateDomPointer(node) {
  if ((node = node._parent) != null && node._component != null) {
    node._dom = node._component.base = null;
    for (let index = 0; index < node._children.length; index++) {
      let child = node._children[index];
      if (child != null && child._dom != null) {
        node._dom = node._component.base = child._dom;
        break;
      }
    }
    return propagateDomPointer(node);
  }
}
Component.prototype.setState = function (newStateOrUpdater, callback) {
  let nextState;
  if (this._nextState != null && this._nextState !== this.state) {
    nextState = this._nextState;
  } else {
    nextState = this._nextState = mergeObjects({}, this.state);
  }
  if (typeof newStateOrUpdater == "function") {
    newStateOrUpdater = newStateOrUpdater(
      mergeObjects({}, nextState),
      this.props
    );
  }
  if (newStateOrUpdater) {
    mergeObjects(nextState, newStateOrUpdater);
  }
  if (newStateOrUpdater != null && this._vnode) {
    if (callback) {
      this._stateCallbacks.push(callback);
    }
    scheduleComponentRender(this);
  }
};
Component.prototype.forceUpdate = function (callback) {
  if (this._vnode) {
    this._force = true;
    if (callback) {
      this._renderCallbacks.push(callback);
    }
    scheduleComponentRender(this);
  }
};
Component.prototype.render = getFragmentChildren;
var previousDebounceFn;
var pendingRerenderQueue = [];
var scheduleTask =
  typeof Promise == "function"
    ? Promise.prototype.then.bind(Promise.resolve())
    : setTimeout;
function scheduleComponentRender(component) {
  if (
    (!component._dirty &&
      (component._dirty = true) &&
      pendingRerenderQueue.push(component) &&
      !handleRerenderQueue._rerenderCount++) ||
    previousDebounceFn !== defaultOptions.debounceRendering
  ) {
    ((previousDebounceFn = defaultOptions.debounceRendering) || scheduleTask)(
      handleRerenderQueue
    );
  }
}
var depthComparator = (firstNode, secondNode) =>
  firstNode._vnode._depth - secondNode._vnode._depth;
function handleRerenderQueue() {
  let component;
  for (
    pendingRerenderQueue.sort(depthComparator);
    (component = pendingRerenderQueue.shift());

  ) {
    if (component._dirty) {
      let previousLength = pendingRerenderQueue.length;
      renderComponentInstance(component);
      if (pendingRerenderQueue.length > previousLength) {
        pendingRerenderQueue.sort(depthComparator);
      }
    }
  }
  handleRerenderQueue._rerenderCount = 0;
}
function diffChildrenNodes(
  diffContext,
  newChildren,
  parentVNode,
  prevVNode,
  parentDom,
  nextDom,
  nextSibling,
  nextIndex,
  nextNode,
  nextRef,
  refUpdates
) {
  let childIndex, prevChild, currentChild, childDom, firstChildDom;
  let prevChildrenArray = (prevVNode && prevVNode._children) || EMPTY_ARRAY;
  let newChildrenCount = newChildren.length;
  for (
    parentVNode._nextDom = nextNode,
      buildChildrenArray(parentVNode, newChildren, prevChildrenArray),
      nextNode = parentVNode._nextDom,
      childIndex = 0;
    childIndex < newChildrenCount;
    childIndex++
  ) {
    currentChild = parentVNode._children[childIndex];
    if (currentChild != null) {
      if (-1 === currentChild._index) {
        prevChild = EMPTY_OBJECT;
      } else {
        prevChild = prevChildrenArray[currentChild._index] || EMPTY_OBJECT;
      }
      currentChild._index = childIndex;
      diffVirtualNodes(
        diffContext,
        currentChild,
        prevChild,
        parentDom,
        nextDom,
        nextSibling,
        nextIndex,
        nextNode,
        nextRef,
        refUpdates
      );
      childDom = currentChild._dom;
      if (currentChild.ref && prevChild.ref != currentChild.ref) {
        if (prevChild.ref) {
          applyReference(prevChild.ref, null, currentChild);
        }
        refUpdates.push(
          currentChild.ref,
          currentChild._component || childDom,
          currentChild
        );
      }
      if (firstChildDom == null && childDom != null) {
        firstChildDom = childDom;
      }
      if (
        currentChild._flags & INSERT_VNODE_FLAG ||
        prevChild._children === currentChild._children
      ) {
        nextNode = insertNode(currentChild, nextNode, diffContext);
      } else if (
        typeof currentChild.type == "function" &&
        currentChild._nextDom !== UNDEFINED_VALUE
      ) {
        nextNode = currentChild._nextDom;
      } else if (childDom) {
        nextNode = childDom.nextSibling;
      }
      currentChild._nextDom = UNDEFINED_VALUE;
      currentChild._flags &= ~(INSERT_VNODE_FLAG | MATCHED_FLAG);
    }
  }
  parentVNode._nextDom = nextNode;
  parentVNode._dom = firstChildDom;
}
function buildChildrenArray(parentVNode, newChildren, oldChildren) {
  let newIndex, newChild, matchedOldChild;
  const index = newChildren.length;
  let oldChildrenCount = oldChildren.length;
  let remainingOldChildren = oldChildrenCount;
  let indexOffset = 0;
  for (parentVNode._children = [], newIndex = 0; newIndex < index; newIndex++) {
    newChild = newChildren[newIndex];
    if (
      newChild == null ||
      typeof newChild == "boolean" ||
      typeof newChild == "function"
    ) {
      newChild = parentVNode._children[newIndex] = null;
      continue;
    }
    if (
      typeof newChild == "string" ||
      typeof newChild == "number" ||
      typeof newChild == "bigint" ||
      newChild.constructor == String
    ) {
      newChild = parentVNode._children[newIndex] = createVirtualNode(
        null,
        newChild,
        null,
        null,
        null
      );
    } else if (isArrayFn(newChild)) {
      newChild = parentVNode._children[newIndex] = createVirtualNode(
        getFragmentChildren,
        {
          children: newChild
        },
        null,
        null,
        null
      );
    } else if (
      newChild.constructor === UNDEFINED_VALUE &&
      newChild._depth > 0
    ) {
      newChild = parentVNode._children[newIndex] = createVirtualNode(
        newChild.type,
        newChild.props,
        newChild.key,
        newChild.ref ? newChild.ref : null,
        newChild._original
      );
    } else {
      newChild = parentVNode._children[newIndex] = newChild;
    }
    const i = newIndex + indexOffset;
    newChild._parent = parentVNode;
    newChild._depth = parentVNode._depth + 1;
    const a = (newChild._index = findMatchingSiblingIndex(
      newChild,
      oldChildren,
      i,
      remainingOldChildren
    ));
    matchedOldChild = null;
    if (-1 !== a) {
      matchedOldChild = oldChildren[a];
      remainingOldChildren--;
      if (matchedOldChild) {
        matchedOldChild._flags |= MATCHED_FLAG;
      }
    }
    if (matchedOldChild == null || matchedOldChild._original === null) {
      if (-1 == a) {
        indexOffset--;
      }
      if (typeof newChild.type != "function") {
        newChild._flags |= INSERT_VNODE_FLAG;
      }
    } else if (a !== i) {
      if (a == i - 1) {
        indexOffset--;
      } else if (a == i + 1) {
        indexOffset++;
      } else {
        if (a > i) {
          indexOffset--;
        } else {
          indexOffset++;
        }
        newChild._flags |= INSERT_VNODE_FLAG;
      }
    }
  }
  if (remainingOldChildren) {
    for (newIndex = 0; newIndex < oldChildrenCount; newIndex++) {
      matchedOldChild = oldChildren[newIndex];
      if (
        matchedOldChild != null &&
        (matchedOldChild._flags & MATCHED_FLAG) === 0
      ) {
        if (matchedOldChild._dom == parentVNode._nextDom) {
          parentVNode._nextDom = getNextDomSibling(matchedOldChild);
        }
        unmountComponent(matchedOldChild, matchedOldChild);
      }
    }
  }
}
function insertNode(vnode, referenceDom, parentDom) {
  if (typeof vnode.type == "function") {
    let children = vnode._children;
    for (let index = 0; children && index < children.length; index++) {
      if (children[index]) {
        children[index]._parent = vnode;
        referenceDom = insertNode(children[index], referenceDom, parentDom);
      }
    }
    return referenceDom;
  }
  if (vnode._dom != referenceDom) {
    if (referenceDom && vnode.type && !parentDom.contains(referenceDom)) {
      referenceDom = getNextDomSibling(vnode);
    }
    parentDom.insertBefore(vnode._dom, referenceDom || null);
    referenceDom = vnode._dom;
  }
  do {
    referenceDom = referenceDom && referenceDom.nextSibling;
  } while (referenceDom != null && referenceDom.nodeType === 8);
  return referenceDom;
}
function gatherChildNodes(nodeOrNodes, childArray) {
  childArray = childArray || [];
  if (!(nodeOrNodes == null || typeof nodeOrNodes == "boolean")) {
    if (isArrayFn(nodeOrNodes)) {
      nodeOrNodes.some((node) => {
        gatherChildNodes(node, childArray);
      });
    } else {
      childArray.push(nodeOrNodes);
    }
  }
  return childArray;
}
function findMatchingSiblingIndex(
  childVNode,
  siblingArray,
  currentIndex,
  currentDepth
) {
  const childKey = childVNode.key;
  const childType = childVNode.type;
  let leftIndex = currentIndex - 1;
  let rightIndex = currentIndex + 1;
  let currentSibling = siblingArray[currentIndex];
  let shouldSearchOutward =
    (typeof childType != "function" ||
      childType === getFragmentChildren ||
      childKey) &&
    currentDepth >
      (currentSibling != null && (currentSibling._flags & MATCHED_FLAG) === 0
        ? 1
        : 0);
  if (
    currentSibling === null ||
    (currentSibling &&
      childKey == currentSibling.key &&
      childType === currentSibling.type &&
      (currentSibling._flags & MATCHED_FLAG) === 0)
  ) {
    return currentIndex;
  }
  if (shouldSearchOutward) {
    for (; leftIndex >= 0 || rightIndex < siblingArray.length; ) {
      if (leftIndex >= 0) {
        currentSibling = siblingArray[leftIndex];
        if (
          currentSibling &&
          (currentSibling._flags & MATCHED_FLAG) === 0 &&
          childKey == currentSibling.key &&
          childType === currentSibling.type
        ) {
          return leftIndex;
        }
        leftIndex--;
      }
      if (rightIndex < siblingArray.length) {
        currentSibling = siblingArray[rightIndex];
        if (
          currentSibling &&
          (currentSibling._flags & MATCHED_FLAG) === 0 &&
          childKey == currentSibling.key &&
          childType === currentSibling.type
        ) {
          return rightIndex;
        }
        rightIndex++;
      }
    }
  }
  return -1;
}
function applyStyle(styleObj, propertyName, value) {
  if (propertyName[0] === "-") {
    styleObj.setProperty(propertyName, value == null ? "" : value);
  } else if (value == null) {
    styleObj[propertyName] = "";
  } else if (
    typeof value != "number" ||
    NON_DIMENSIONAL_REGEX.test(propertyName)
  ) {
    styleObj[propertyName] = value;
  } else {
    styleObj[propertyName] = value + "px";
  }
}
handleRerenderQueue._rerenderCount = 0;
var eventCounter = 0;
function setElementProperty(
  element,
  propertyName,
  newValue,
  oldValue,
  namespace
) {
  let isCapture;
  e: if (propertyName === "style") {
    if (typeof newValue == "string") {
      element.style.cssText = newValue;
    } else {
      if (typeof oldValue == "string") {
        element.style.cssText = oldValue = "";
      }
      if (oldValue) {
        for (propertyName in oldValue)
          if (!(newValue && propertyName in newValue)) {
            applyStyle(element.style, propertyName, "");
          }
      }
      if (newValue) {
        for (propertyName in newValue)
          if (
            !(oldValue && newValue[propertyName] === oldValue[propertyName])
          ) {
            applyStyle(element.style, propertyName, newValue[propertyName]);
          }
      }
    }
  } else if (propertyName[0] === "o" && propertyName[1] === "n") {
    isCapture =
      propertyName !==
      (propertyName = propertyName.replace(
        /(PointerCapture)$|Capture$/i,
        "$1"
      ));
    if (
      propertyName.toLowerCase() in element ||
      propertyName === "onFocusOut" ||
      propertyName === "onFocusIn"
    ) {
      propertyName = propertyName.toLowerCase().slice(2);
    } else {
      propertyName = propertyName.slice(2);
    }
    if (!element._listeners) {
      element._listeners = {};
    }
    element._listeners[propertyName + isCapture] = newValue;
    if (newValue) {
      if (oldValue) {
        newValue._attached = oldValue._attached;
      } else {
        newValue._attached = eventCounter;
        element.addEventListener(
          propertyName,
          isCapture ? captureEventHandler : nonCaptureEventHandler,
          isCapture
        );
      }
    } else {
      element.removeEventListener(
        propertyName,
        isCapture ? captureEventHandler : nonCaptureEventHandler,
        isCapture
      );
    }
  } else {
    if (namespace == "http://www.w3.org/2000/svg") {
      propertyName = propertyName
        .replace(/xlink(H|:h)/, "h")
        .replace(/sName$/, "s");
    } else if (
      propertyName != "width" &&
      propertyName != "height" &&
      propertyName != "href" &&
      propertyName != "list" &&
      propertyName != "form" &&
      propertyName != "tabIndex" &&
      propertyName != "download" &&
      propertyName != "rowSpan" &&
      propertyName != "colSpan" &&
      propertyName != "role" &&
      propertyName != "popover" &&
      propertyName in element
    ) {
      try {
        element[propertyName] = newValue == null ? "" : newValue;
        break e;
      } catch (propertySetBlock) {}
    }
    if (!(typeof newValue == "function")) {
      if (newValue == null || (newValue === false && propertyName[4] !== "-")) {
        element.removeAttribute(propertyName);
      } else {
        element.setAttribute(
          propertyName,
          propertyName == "popover" && newValue == 1 ? "" : newValue
        );
      }
    }
  }
}
function createEventHandlerProxy(eventSuffix) {
  return function (event) {
    if (this._listeners) {
      const listener = this._listeners[event.type + eventSuffix];
      if (event._dispatched == null) {
        event._dispatched = eventCounter++;
      } else if (event._dispatched < listener._attached) {
        return;
      }
      if (defaultOptions.event) {
        event = defaultOptions.event(event);
      }
      if ("handleEvent" in listener) {
        return listener.handleEvent(event);
      } else {
        return listener(event);
      }
    }
  };
}
var nonCaptureEventHandler = createEventHandlerProxy(false);
var captureEventHandler = createEventHandlerProxy(true);
function diffVirtualNodes(
  parentDom,
  vnode,
  oldVnode,
  context,
  namespaceURI,
  suspendedNodes,
  pendingComponents,
  hydrationNode,
  isHydrating,
  parentComponent
) {
  let renderResult;
  let componentType = vnode.type;
  if (vnode.constructor !== UNDEFINED_VALUE) {
    return null;
  }
  if (oldVnode._flags & SUSPENDED_MODE) {
    isHydrating = !!(oldVnode._flags & HYDRATE_MODE);
    suspendedNodes = [(hydrationNode = vnode._dom = oldVnode._dom)];
  }
  if ((renderResult = defaultOptions._diff)) {
    renderResult(vnode);
  }
  e: if (typeof componentType == "function") {
    try {
      let componentInstance,
        newVnode,
        currentProps,
        currentState,
        snapshotBeforeUpdateResult,
        pendingError;
      let initialProps = vnode.props;
      const isClassComponent =
        "prototype" in componentType && componentType.prototype.render;
      renderResult = componentType.contextType;
      let contextProvider = renderResult && context[renderResult._id];
      let contextValue = renderResult
        ? contextProvider
          ? contextProvider.props.value
          : renderResult._defaultValue
        : context;
      if (oldVnode._component) {
        componentInstance = vnode._component = oldVnode._component;
        pendingError = componentInstance._processingException =
          componentInstance._pendingError;
      } else {
        if (isClassComponent) {
          vnode._component = componentInstance = new componentType(
            initialProps,
            contextValue
          );
        } else {
          vnode._component = componentInstance = new Component(
            initialProps,
            contextValue
          );
          componentInstance.constructor = componentType;
          componentInstance.render = createInstance;
        }
        if (contextProvider) {
          contextProvider.sub(componentInstance);
        }
        componentInstance.props = initialProps;
        if (!componentInstance.state) {
          componentInstance.state = {};
        }
        componentInstance.context = contextValue;
        componentInstance._globalContext = context;
        newVnode = componentInstance._dirty = true;
        componentInstance._renderCallbacks = [];
        componentInstance._stateCallbacks = [];
      }
      if (isClassComponent && componentInstance._nextState == null) {
        componentInstance._nextState = componentInstance.state;
      }
      if (isClassComponent && componentType.getDerivedStateFromProps != null) {
        if (componentInstance._nextState == componentInstance.state) {
          componentInstance._nextState = mergeObjects(
            {},
            componentInstance._nextState
          );
        }
        mergeObjects(
          componentInstance._nextState,
          componentType.getDerivedStateFromProps(
            initialProps,
            componentInstance._nextState
          )
        );
      }
      currentProps = componentInstance.props;
      currentState = componentInstance.state;
      componentInstance._vnode = vnode;
      if (newVnode) {
        if (
          isClassComponent &&
          componentType.getDerivedStateFromProps == null &&
          componentInstance.componentWillMount != null
        ) {
          componentInstance.componentWillMount();
        }
        if (isClassComponent && componentInstance.componentDidMount != null) {
          componentInstance._renderCallbacks.push(
            componentInstance.componentDidMount
          );
        }
      } else {
        if (
          isClassComponent &&
          componentType.getDerivedStateFromProps == null &&
          initialProps !== currentProps &&
          componentInstance.componentWillReceiveProps != null
        ) {
          componentInstance.componentWillReceiveProps(
            initialProps,
            contextValue
          );
        }
        if (
          !componentInstance._force &&
          ((componentInstance.shouldComponentUpdate != null &&
            false ===
              componentInstance.shouldComponentUpdate(
                initialProps,
                componentInstance._nextState,
                contextValue
              )) ||
            vnode._original === oldVnode._original)
        ) {
          if (vnode._original !== oldVnode._original) {
            componentInstance.props = initialProps;
            componentInstance.state = componentInstance._nextState;
            componentInstance._dirty = false;
          }
          vnode._dom = oldVnode._dom;
          vnode._children = oldVnode._children;
          vnode._children.some((childNode) => {
            if (childNode) {
              childNode._parent = vnode;
            }
          });
          for (let e = 0; e < componentInstance._stateCallbacks.length; e++) {
            componentInstance._renderCallbacks.push(
              componentInstance._stateCallbacks[e]
            );
          }
          componentInstance._stateCallbacks = [];
          if (componentInstance._renderCallbacks.length) {
            pendingComponents.push(componentInstance);
          }
          break e;
        }
        if (componentInstance.componentWillUpdate != null) {
          componentInstance.componentWillUpdate(
            initialProps,
            componentInstance._nextState,
            contextValue
          );
        }
        if (isClassComponent && componentInstance.componentDidUpdate != null) {
          componentInstance._renderCallbacks.push(() => {
            componentInstance.componentDidUpdate(
              currentProps,
              currentState,
              snapshotBeforeUpdateResult
            );
          });
        }
      }
      componentInstance.context = contextValue;
      componentInstance.props = initialProps;
      componentInstance._parentDom = parentDom;
      componentInstance._force = false;
      let renderHook = defaultOptions._render;
      let renderLoopCounter = 0;
      if (isClassComponent) {
        componentInstance.state = componentInstance._nextState;
        componentInstance._dirty = false;
        if (renderHook) {
          renderHook(vnode);
        }
        renderResult = componentInstance.render(
          componentInstance.props,
          componentInstance.state,
          componentInstance.context
        );
        for (let e = 0; e < componentInstance._stateCallbacks.length; e++) {
          componentInstance._renderCallbacks.push(
            componentInstance._stateCallbacks[e]
          );
        }
        componentInstance._stateCallbacks = [];
      } else {
        do {
          componentInstance._dirty = false;
          if (renderHook) {
            renderHook(vnode);
          }
          renderResult = componentInstance.render(
            componentInstance.props,
            componentInstance.state,
            componentInstance.context
          );
          componentInstance.state = componentInstance._nextState;
        } while (componentInstance._dirty && ++renderLoopCounter < 25);
      }
      componentInstance.state = componentInstance._nextState;
      if (componentInstance.getChildContext != null) {
        context = mergeObjects(
          mergeObjects({}, context),
          componentInstance.getChildContext()
        );
      }
      if (
        isClassComponent &&
        !newVnode &&
        componentInstance.getSnapshotBeforeUpdate != null
      ) {
        snapshotBeforeUpdateResult = componentInstance.getSnapshotBeforeUpdate(
          currentProps,
          currentState
        );
      }
      let renderedNode =
        renderResult != null &&
        renderResult.type === getFragmentChildren &&
        renderResult.key == null
          ? renderResult.props.children
          : renderResult;
      diffChildrenNodes(
        parentDom,
        isArrayFn(renderedNode) ? renderedNode : [renderedNode],
        vnode,
        oldVnode,
        context,
        namespaceURI,
        suspendedNodes,
        pendingComponents,
        hydrationNode,
        isHydrating,
        parentComponent
      );
      componentInstance.base = vnode._dom;
      vnode._flags &= RESET_FLAGS_MASK;
      if (componentInstance._renderCallbacks.length) {
        pendingComponents.push(componentInstance);
      }
      if (pendingError) {
        componentInstance._pendingError =
          componentInstance._processingException = null;
      }
    } catch (componentLoop) {
      vnode._original = null;
      if (isHydrating || suspendedNodes != null) {
        for (
          vnode._flags |= isHydrating
            ? HYDRATE_MODE | SUSPENDED_MODE
            : SUSPENDED_MODE;
          hydrationNode &&
          hydrationNode.nodeType === 8 &&
          hydrationNode.nextSibling;

        ) {
          hydrationNode = hydrationNode.nextSibling;
        }
        suspendedNodes[suspendedNodes.indexOf(hydrationNode)] = null;
        vnode._dom = hydrationNode;
      } else {
        vnode._dom = oldVnode._dom;
        vnode._children = oldVnode._children;
      }
      defaultOptions._catchError(componentLoop, vnode, oldVnode);
    }
  } else if (suspendedNodes == null && vnode._original === oldVnode._original) {
    vnode._children = oldVnode._children;
    vnode._dom = oldVnode._dom;
  } else {
    vnode._dom = diffElementNode(
      oldVnode._dom,
      vnode,
      oldVnode,
      context,
      namespaceURI,
      suspendedNodes,
      pendingComponents,
      isHydrating,
      parentComponent
    );
  }
  if ((renderResult = defaultOptions.diffed)) {
    renderResult(vnode);
  }
}
function commitRootComponent(root, fiber, references) {
  fiber._nextDom = UNDEFINED_VALUE;
  for (let e = 0; e < references.length; e++) {
    applyReference(references[e], references[++e], references[++e]);
  }
  if (defaultOptions._commit) {
    defaultOptions._commit(fiber, root);
  }
  root.some((componentInstance) => {
    try {
      root = componentInstance._renderCallbacks;
      componentInstance._renderCallbacks = [];
      root.some((callback) => {
        callback.call(componentInstance);
      });
    } catch (error) {
      defaultOptions._catchError(error, componentInstance._vnode);
    }
  });
}
function diffElementNode(
  domNode,
  newVNode,
  oldVNode,
  parent,
  namespace,
  existingChildren,
  index,
  hydration,
  isForeignObject
) {
  let key,
    newDangerouslySetInnerHTML,
    oldDangerouslySetInnerHTML,
    newChildren,
    nodeOrAttribute,
    newValue,
    newChecked;
  let oldProps = oldVNode.props;
  let newProps = newVNode.props;
  let newType = newVNode.type;
  if (newType === "svg") {
    namespace = "http://www.w3.org/2000/svg";
  } else if (newType === "math") {
    namespace = "http://www.w3.org/1998/Math/MathML";
  } else if (!namespace) {
    namespace = "http://www.w3.org/1999/xhtml";
  }
  if (existingChildren != null) {
    for (key = 0; key < existingChildren.length; key++) {
      nodeOrAttribute = existingChildren[key];
      if (
        nodeOrAttribute &&
        "setAttribute" in nodeOrAttribute == !!newType &&
        (newType
          ? nodeOrAttribute.localName === newType
          : nodeOrAttribute.nodeType === 3)
      ) {
        domNode = nodeOrAttribute;
        existingChildren[key] = null;
        break;
      }
    }
  }
  if (domNode == null) {
    if (newType === null) {
      return document.createTextNode(newProps);
    }
    domNode = document.createElementNS(
      namespace,
      newType,
      newProps.is && newProps
    );
    if (hydration) {
      if (defaultOptions._hydrationMismatch) {
        defaultOptions._hydrationMismatch(newVNode, existingChildren);
      }
      hydration = false;
    }
    existingChildren = null;
  }
  if (newType === null) {
    if (!(oldProps === newProps || (hydration && domNode.data === newProps))) {
      domNode.data = newProps;
    }
  } else {
    existingChildren = existingChildren && arraySlice.call(domNode.childNodes);
    oldProps = oldVNode.props || EMPTY_OBJECT;
    if (!hydration && existingChildren != null) {
      for (oldProps = {}, key = 0; key < domNode.attributes.length; key++) {
        nodeOrAttribute = domNode.attributes[key];
        oldProps[nodeOrAttribute.name] = nodeOrAttribute.value;
      }
    }
    for (key in oldProps) {
      nodeOrAttribute = oldProps[key];
      if (key == "children") {
      } else if (key == "dangerouslySetInnerHTML") {
        oldDangerouslySetInnerHTML = nodeOrAttribute;
      } else if (!(key in newProps)) {
        if (
          (key == "value" && "defaultValue" in newProps) ||
          (key == "checked" && "defaultChecked" in newProps)
        ) {
          continue;
        }
        setElementProperty(domNode, key, null, nodeOrAttribute, namespace);
      }
    }
    for (key in newProps) {
      nodeOrAttribute = newProps[key];
      if (key == "children") {
        newChildren = nodeOrAttribute;
      } else if (key == "dangerouslySetInnerHTML") {
        newDangerouslySetInnerHTML = nodeOrAttribute;
      } else if (key == "value") {
        newValue = nodeOrAttribute;
      } else if (key == "checked") {
        newChecked = nodeOrAttribute;
      } else if (
        !(
          (hydration && typeof nodeOrAttribute != "function") ||
          oldProps[key] === nodeOrAttribute
        )
      ) {
        setElementProperty(
          domNode,
          key,
          nodeOrAttribute,
          oldProps[key],
          namespace
        );
      }
    }
    if (newDangerouslySetInnerHTML) {
      if (
        !(
          hydration ||
          (oldDangerouslySetInnerHTML &&
            (newDangerouslySetInnerHTML.__html ===
              oldDangerouslySetInnerHTML.__html ||
              newDangerouslySetInnerHTML.__html === domNode.innerHTML))
        )
      ) {
        domNode.innerHTML = newDangerouslySetInnerHTML.__html;
      }
      newVNode._children = [];
    } else {
      if (oldDangerouslySetInnerHTML) {
        domNode.innerHTML = "";
      }
      diffChildrenNodes(
        domNode,
        isArrayFn(newChildren) ? newChildren : [newChildren],
        newVNode,
        oldVNode,
        parent,
        newType === "foreignObject"
          ? "http://www.w3.org/1999/xhtml"
          : namespace,
        existingChildren,
        index,
        existingChildren
          ? existingChildren[0]
          : oldVNode._children && getNextDomSibling(oldVNode, 0),
        hydration,
        isForeignObject
      );
      if (existingChildren != null) {
        for (key = existingChildren.length; key--; ) {
          removeDomNode(existingChildren[key]);
        }
      }
    }
    if (!hydration) {
      key = "value";
      if (newType === "progress" && newValue == null) {
        domNode.removeAttribute("value");
      } else if (
        newValue !== UNDEFINED_VALUE &&
        (newValue !== domNode[key] ||
          (newType === "progress" && !newValue) ||
          (newType === "option" && newValue !== oldProps[key]))
      ) {
        setElementProperty(domNode, key, newValue, oldProps[key], namespace);
      }
      key = "checked";
      if (newChecked !== UNDEFINED_VALUE && newChecked !== domNode[key]) {
        setElementProperty(domNode, key, newChecked, oldProps[key], namespace);
      }
    }
  }
  return domNode;
}
function applyReference(refOrCallback, value, config) {
  try {
    if (typeof refOrCallback == "function") {
      let n = typeof refOrCallback._unmount == "function";
      if (n) {
        refOrCallback._unmount();
      }
      if (!(n && value == null)) {
        refOrCallback._unmount = refOrCallback(value);
      }
    } else {
      refOrCallback.current = value;
    }
  } catch (error) {
    defaultOptions._catchError(error, config);
  }
}
function unmountComponent(component, config, isNativeElement) {
  let temp;
  if (defaultOptions.unmount) {
    defaultOptions.unmount(component);
  }
  if ((temp = component.ref)) {
    if (!(temp.current && temp.current !== component._dom)) {
      applyReference(temp, null, config);
    }
  }
  if ((temp = component._component) != null) {
    if (temp.componentWillUnmount) {
      try {
        temp.componentWillUnmount();
      } catch (targetComponent) {
        defaultOptions._catchError(targetComponent, config);
      }
    }
    temp.base = temp._parentDom = null;
  }
  if ((temp = component._children)) {
    for (let childIndex = 0; childIndex < temp.length; childIndex++) {
      if (temp[childIndex]) {
        unmountComponent(
          temp[childIndex],
          config,
          isNativeElement || typeof component.type != "function"
        );
      }
    }
  }
  if (!isNativeElement) {
    removeDomNode(component._dom);
  }
  component._component =
    component._parent =
    component._dom =
    component._nextDom =
      UNDEFINED_VALUE;
}
function createInstance(element, context, config) {
  return this.constructor(element, config);
}
function renderRoot(rootElement, container, hydrateOption) {
  if (defaultOptions._root) {
    defaultOptions._root(rootElement, container);
  }
  let isHydrateFunction = typeof hydrateOption == "function";
  let childNodes = isHydrateFunction
    ? null
    : (hydrateOption && hydrateOption._children) || container._children;
  let referenceList = [];
  let pendingReferenceList = [];
  diffVirtualNodes(
    container,
    (rootElement = (
      (!isHydrateFunction && hydrateOption) ||
      container
    )._children =
      createVirtualElement(getFragmentChildren, null, [rootElement])),
    childNodes || EMPTY_OBJECT,
    EMPTY_OBJECT,
    container.namespaceURI,
    !isHydrateFunction && hydrateOption
      ? [hydrateOption]
      : childNodes
        ? null
        : container.firstChild
          ? arraySlice.call(container.childNodes)
          : null,
    referenceList,
    !isHydrateFunction && hydrateOption
      ? hydrateOption
      : childNodes
        ? childNodes._dom
        : container.firstChild,
    isHydrateFunction,
    pendingReferenceList
  );
  commitRootComponent(referenceList, rootElement, pendingReferenceList);
}
function hydrateComponent(root, options) {
  renderRoot(root, options, hydrateComponent);
}
function cloneVirtualElement(originalElement, newProps, children) {
  let key, ref, propKey, defaultProps;
  let mergedProps = mergeObjects({}, originalElement.props);
  if (originalElement.type && originalElement.type.defaultProps) {
    defaultProps = originalElement.type.defaultProps;
  }
  for (propKey in newProps)
    if (propKey == "key") {
      key = newProps[propKey];
    } else if (propKey == "ref") {
      ref = newProps[propKey];
    } else if (
      newProps[propKey] === UNDEFINED_VALUE &&
      defaultProps !== UNDEFINED_VALUE
    ) {
      mergedProps[propKey] = defaultProps[propKey];
    } else {
      mergedProps[propKey] = newProps[propKey];
    }
  if (arguments.length > 2) {
    mergedProps.children =
      arguments.length > 3 ? arraySlice.call(arguments, 2) : children;
  }
  return createVirtualNode(
    originalElement.type,
    mergedProps,
    key || originalElement.key,
    ref || originalElement.ref,
    null
  );
}
var props = 0;
function createContextObject(defaultValue, contextKey) {
  const context = {
    _id: (contextKey = "__cC" + props++),
    _defaultValue: defaultValue,
    Consumer: (parentElement, childSelector) =>
      parentElement.children(childSelector),
    Provider(componentOrSet) {
      if (!this.getChildContext) {
        let e = new Set();
        let childContextObj = {};
        childContextObj[contextKey] = this;
        this.getChildContext = () => childContextObj;
        this.componentWillUnmount = () => {
          e = null;
        };
        this.shouldComponentUpdate = function (nextProps) {
          if (this.props.value !== nextProps.value) {
            e.forEach((component) => {
              component._force = true;
              scheduleComponentRender(component);
            });
          }
        };
        this.sub = (componentInstance) => {
          e.add(componentInstance);
          let originalUnmount = componentInstance.componentWillUnmount;
          componentInstance.componentWillUnmount = () => {
            if (e) {
              e.delete(componentInstance);
            }
            if (originalUnmount) {
              originalUnmount.call(componentInstance);
            }
          };
        };
      }
      return componentOrSet.children;
    }
  };
  return (context.Provider._contextRef = context.Consumer.contextType =
    context);
}
export {
  Component,
  getFragmentChildren as Fragment,
  cloneVirtualElement as cloneElement,
  createContextObject as createContext,
  createVirtualElement as createElement,
  createRefObject as createRef,
  createVirtualElement as h,
  hydrateComponent as hydrate,
  isValidReactElement as isValidElement,
  defaultOptions as options,
  renderRoot as render,
  gatherChildNodes as toChildArray
};
