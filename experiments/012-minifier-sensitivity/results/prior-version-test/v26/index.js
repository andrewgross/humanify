var HYDRATE_MODE = 32;
var SUSPENDED_MODE = 128;
var INSERT_VNODE_FLAG = 4;
var MATCHED_FLAG = 2;
var RESET_FLAGS_MASK = ~(HYDRATE_MODE | SUSPENDED_MODE);
var SVG_NS = "http://www.w3.org/2000/svg";
var XHTML_NS = "http://www.w3.org/1999/xhtml";
var MATH_NS = "http://www.w3.org/1998/Math/MathML";
var UNDEFINED_VALUE = undefined;
var EMPTY_OBJECT = {};
var EMPTY_ARRAY = [];
var NON_DIMENSIONAL_CSS_REGEX =
  /acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i;
var isArrayCheck = Array.isArray;
function mergeObjects(targetObj, sourceObj) {
  for (let n in sourceObj) targetObj[n] = sourceObj[n];
  return targetObj;
}
function removeDomNode(node) {
  if (node && node.parentNode) {
    node.parentNode.removeChild(node);
  }
}
var arraySlice = EMPTY_ARRAY.slice;
function handleCatchError(error, currentComponent, unusedParam, errorInfo) {
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
      } catch (nestedError) {
        error = nestedError;
      }
    }
  }
  throw error;
}
var baseOptions = {
  _catchError: handleCatchError
};
var defaultOptions = baseOptions;
var vnodeIdCounter = 0;
function createVirtualElement(componentType, props, child) {
  let key, ref, propName;
  let processedProps = {};
  for (propName in props)
    if (propName == "key") {
      key = props[propName];
    } else if (propName == "ref") {
      ref = props[propName];
    } else {
      processedProps[propName] = props[propName];
    }
  if (arguments.length > 2) {
    processedProps.children =
      arguments.length > 3 ? arraySlice.call(arguments, 2) : child;
  }
  if (
    typeof componentType == "function" &&
    componentType.defaultProps != null
  ) {
    for (propName in componentType.defaultProps)
      if (processedProps[propName] === UNDEFINED_VALUE) {
        processedProps[propName] = componentType.defaultProps[propName];
      }
  }
  return createVirtualNode(componentType, processedProps, key, ref, null);
}
function createVirtualNode(type, props, key, ref, originalId) {
  const vnode = {
    type: type,
    props: props,
    key: key,
    ref: ref,
    _children: null,
    _parent: null,
    _depth: 0,
    _dom: null,
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
function createReferenceObject() {
  return {
    current: null
  };
}
function getChildrenFromNode(node) {
  return node.children;
}
var isValidComponentInstance = (value) =>
  value != null && value.constructor == UNDEFINED_VALUE;
function ComponentBase(props, context) {
  this.props = props;
  this.context = context;
}
function findNextDomSibling(vnode, startIndex) {
  if (startIndex == null) {
    if (vnode._parent) {
      return findNextDomSibling(vnode._parent, vnode._index + 1);
    } else {
      return null;
    }
  }
  let childNode;
  for (; startIndex < vnode._children.length; startIndex++) {
    childNode = vnode._children[startIndex];
    if (childNode != null && childNode._dom != null) {
      return childNode._dom;
    }
  }
  if (typeof vnode.type == "function") {
    return findNextDomSibling(vnode);
  } else {
    return null;
  }
}
function renderComponentInstance(componentInstance) {
  let vnode = componentInstance._vnode;
  let domNode = vnode._dom;
  let fiberArray = [];
  let refArray = [];
  if (componentInstance._parentDom) {
    const mergedVnode = mergeObjects({}, vnode);
    mergedVnode._original = vnode._original + 1;
    if (defaultOptions.vnode) {
      defaultOptions.vnode(mergedVnode);
    }
    diffNodes(
      componentInstance._parentDom,
      mergedVnode,
      vnode,
      componentInstance._globalContext,
      componentInstance._parentDom.namespaceURI,
      vnode._flags & HYDRATE_MODE ? [domNode] : null,
      fiberArray,
      domNode == null ? findNextDomSibling(vnode) : domNode,
      !!(vnode._flags & HYDRATE_MODE),
      refArray
    );
    mergedVnode._original = vnode._original;
    mergedVnode._parent._children[mergedVnode._index] = mergedVnode;
    commitRootFiberTree(fiberArray, mergedVnode, refArray);
    if (mergedVnode._dom != domNode) {
      propagateDomPointerToParent(mergedVnode);
    }
  }
}
function propagateDomPointerToParent(parentComponent) {
  if (
    (parentComponent = parentComponent._parent) != null &&
    parentComponent._component != null
  ) {
    parentComponent._dom = parentComponent._component.base = null;
    for (let t = 0; t < parentComponent._children.length; t++) {
      let n = parentComponent._children[t];
      if (n != null && n._dom != null) {
        parentComponent._dom = parentComponent._component.base = n._dom;
        break;
      }
    }
    return propagateDomPointerToParent(parentComponent);
  }
}
ComponentBase.prototype.setState = function (newStateOrUpdater, callback) {
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
    scheduleRender(this);
  }
};
ComponentBase.prototype.forceUpdate = function (renderCallback) {
  if (this._vnode) {
    this._force = true;
    if (renderCallback) {
      this._renderCallbacks.push(renderCallback);
    }
    scheduleRender(this);
  }
};
ComponentBase.prototype.render = getChildrenFromNode;
var previousDebounceFn;
var pendingRerenderQueue = [];
var scheduleDeferred =
  typeof Promise == "function"
    ? Promise.prototype.then.bind(Promise.resolve())
    : setTimeout;
function scheduleRender(component) {
  if (
    (!component._dirty &&
      (component._dirty = true) &&
      pendingRerenderQueue.push(component) &&
      !processRerenderQueue._rerenderCount++) ||
    previousDebounceFn !== defaultOptions.debounceRendering
  ) {
    (
      (previousDebounceFn = defaultOptions.debounceRendering) ||
      scheduleDeferred
    )(processRerenderQueue);
  }
}
var sortByDepth = (nodeA, nodeB) => nodeA._vnode._depth - nodeB._vnode._depth;
function processRerenderQueue() {
  let component;
  let remainingQueueLength = 1;
  for (; pendingRerenderQueue.length; ) {
    if (pendingRerenderQueue.length > remainingQueueLength) {
      pendingRerenderQueue.sort(sortByDepth);
    }
    component = pendingRerenderQueue.shift();
    remainingQueueLength = pendingRerenderQueue.length;
    if (component._dirty) {
      renderComponentInstance(component);
    }
  }
  processRerenderQueue._rerenderCount = 0;
}
function diffChildrenNodes(
  parentDom,
  newChildren,
  parentVNode,
  oldVNode,
  owner,
  lastDomNode,
  lastIndex,
  lastAnchorNode,
  nextSiblingNode,
  updateQueue,
  refUpdates
) {
  let childIndex, prevChild, currentChild, childDom, firstDomNode;
  let oldChildren = (oldVNode && oldVNode._children) || EMPTY_ARRAY;
  let newChildrenCount = newChildren.length;
  for (
    nextSiblingNode = reconcileChildren(
      parentVNode,
      newChildren,
      oldChildren,
      nextSiblingNode,
      newChildrenCount
    ),
      childIndex = 0;
    childIndex < newChildrenCount;
    childIndex++
  ) {
    currentChild = parentVNode._children[childIndex];
    if (currentChild == null) {
      continue;
    }
    if (-1 === currentChild._index) {
      prevChild = EMPTY_OBJECT;
    } else {
      prevChild = oldChildren[currentChild._index] || EMPTY_OBJECT;
    }
    currentChild._index = childIndex;
    let childNextDom = diffNodes(
      parentDom,
      currentChild,
      prevChild,
      owner,
      lastDomNode,
      lastIndex,
      lastAnchorNode,
      nextSiblingNode,
      updateQueue,
      refUpdates
    );
    childDom = currentChild._dom;
    if (currentChild.ref && prevChild.ref != currentChild.ref) {
      if (prevChild.ref) {
        setRef(prevChild.ref, null, currentChild);
      }
      refUpdates.push(
        currentChild.ref,
        currentChild._component || childDom,
        currentChild
      );
    }
    if (firstDomNode == null && childDom != null) {
      firstDomNode = childDom;
    }
    if (
      currentChild._flags & INSERT_VNODE_FLAG ||
      prevChild._children === currentChild._children
    ) {
      nextSiblingNode = insertVNode(currentChild, nextSiblingNode, parentDom);
    } else if (
      typeof currentChild.type == "function" &&
      childNextDom !== UNDEFINED_VALUE
    ) {
      nextSiblingNode = childNextDom;
    } else if (childDom) {
      nextSiblingNode = childDom.nextSibling;
    }
    currentChild._flags &= ~(INSERT_VNODE_FLAG | MATCHED_FLAG);
  }
  parentVNode._dom = firstDomNode;
  return nextSiblingNode;
}
function reconcileChildren(
  parentNode,
  newChildrenArray,
  oldChildrenArray,
  unusedParam,
  newChildrenLength
) {
  let newChildIndex, currentChild, matchedSibling;
  let oldChildrenLength = oldChildrenArray.length;
  let remainingOldChildrenCount = oldChildrenLength;
  let indexOffset = 0;
  for (
    parentNode._children = new Array(newChildrenLength), newChildIndex = 0;
    newChildIndex < newChildrenLength;
    newChildIndex++
  ) {
    currentChild = newChildrenArray[newChildIndex];
    if (
      currentChild == null ||
      typeof currentChild == "boolean" ||
      typeof currentChild == "function"
    ) {
      parentNode._children[newChildIndex] = null;
      continue;
    }
    if (
      typeof currentChild == "string" ||
      typeof currentChild == "number" ||
      typeof currentChild == "bigint" ||
      currentChild.constructor == String
    ) {
      currentChild = parentNode._children[newChildIndex] = createVirtualNode(
        null,
        currentChild,
        null,
        null,
        null
      );
    } else if (isArrayCheck(currentChild)) {
      currentChild = parentNode._children[newChildIndex] = createVirtualNode(
        getChildrenFromNode,
        {
          children: currentChild
        },
        null,
        null,
        null
      );
    } else if (
      currentChild.constructor === UNDEFINED_VALUE &&
      currentChild._depth > 0
    ) {
      currentChild = parentNode._children[newChildIndex] = createVirtualNode(
        currentChild.type,
        currentChild.props,
        currentChild.key,
        currentChild.ref ? currentChild.ref : null,
        currentChild._original
      );
    } else {
      currentChild = parentNode._children[newChildIndex] = currentChild;
    }
    const o = newChildIndex + indexOffset;
    currentChild._parent = parentNode;
    currentChild._depth = parentNode._depth + 1;
    const r = (currentChild._index = findMatchingSiblingIndex(
      currentChild,
      oldChildrenArray,
      o,
      remainingOldChildrenCount
    ));
    matchedSibling = null;
    if (-1 !== r) {
      matchedSibling = oldChildrenArray[r];
      remainingOldChildrenCount--;
      if (matchedSibling) {
        matchedSibling._flags |= MATCHED_FLAG;
      }
    }
    if (matchedSibling == null || matchedSibling._original === null) {
      if (-1 == r) {
        indexOffset--;
      }
      if (typeof currentChild.type != "function") {
        currentChild._flags |= INSERT_VNODE_FLAG;
      }
    } else if (r != o) {
      if (r == o - 1) {
        indexOffset--;
      } else if (r == o + 1) {
        indexOffset++;
      } else {
        if (r > o) {
          indexOffset--;
        } else {
          indexOffset++;
        }
        currentChild._flags |= INSERT_VNODE_FLAG;
      }
    }
  }
  if (remainingOldChildrenCount) {
    for (
      newChildIndex = 0;
      newChildIndex < oldChildrenLength;
      newChildIndex++
    ) {
      matchedSibling = oldChildrenArray[newChildIndex];
      if (
        matchedSibling != null &&
        (matchedSibling._flags & MATCHED_FLAG) == 0
      ) {
        if (matchedSibling._dom == unusedParam) {
          unusedParam = findNextDomSibling(matchedSibling);
        }
        unmountComponent(matchedSibling, matchedSibling);
      }
    }
  }
  return unusedParam;
}
function insertVNode(vnode, nextDomSibling, parentDom) {
  if (typeof vnode.type == "function") {
    let children = vnode._children;
    for (let index = 0; children && index < children.length; index++) {
      if (children[index]) {
        children[index]._parent = vnode;
        nextDomSibling = insertVNode(
          children[index],
          nextDomSibling,
          parentDom
        );
      }
    }
    return nextDomSibling;
  }
  if (vnode._dom != nextDomSibling) {
    if (nextDomSibling && vnode.type && !parentDom.contains(nextDomSibling)) {
      nextDomSibling = findNextDomSibling(vnode);
    }
    parentDom.insertBefore(vnode._dom, nextDomSibling || null);
    nextDomSibling = vnode._dom;
  }
  do {
    nextDomSibling = nextDomSibling && nextDomSibling.nextSibling;
  } while (nextDomSibling != null && nextDomSibling.nodeType == 8);
  return nextDomSibling;
}
function flattenNonBooleanElements(node, resultArray) {
  resultArray = resultArray || [];
  if (!(node == null || typeof node == "boolean")) {
    if (isArrayCheck(node)) {
      node.some((child) => {
        flattenNonBooleanElements(child, resultArray);
      });
    } else {
      resultArray.push(node);
    }
  }
  return resultArray;
}
function findMatchingSiblingIndex(
  currentElement,
  siblingArray,
  currentIndex,
  searchDepth
) {
  const elementKey = currentElement.key;
  const elementType = currentElement.type;
  let sibling = siblingArray[currentIndex];
  let shouldSearchFurther =
    searchDepth >
    (sibling != null && (sibling._flags & MATCHED_FLAG) == 0 ? 1 : 0);
  if (
    sibling === null ||
    (sibling &&
      elementKey == sibling.key &&
      elementType === sibling.type &&
      (sibling._flags & MATCHED_FLAG) == 0)
  ) {
    return currentIndex;
  }
  if (shouldSearchFurther) {
    let leftIndex = currentIndex - 1;
    let rightIndex = currentIndex + 1;
    for (; leftIndex >= 0 || rightIndex < siblingArray.length; ) {
      if (leftIndex >= 0) {
        sibling = siblingArray[leftIndex];
        if (
          sibling &&
          (sibling._flags & MATCHED_FLAG) == 0 &&
          elementKey == sibling.key &&
          elementType === sibling.type
        ) {
          return leftIndex;
        }
        leftIndex--;
      }
      if (rightIndex < siblingArray.length) {
        sibling = siblingArray[rightIndex];
        if (
          sibling &&
          (sibling._flags & MATCHED_FLAG) == 0 &&
          elementKey == sibling.key &&
          elementType === sibling.type
        ) {
          return rightIndex;
        }
        rightIndex++;
      }
    }
  }
  return -1;
}
function applyStyle(elementStyle, styleProperty, styleValue) {
  if (styleProperty[0] == "-") {
    elementStyle.setProperty(
      styleProperty,
      styleValue == null ? "" : styleValue
    );
  } else if (styleValue == null) {
    elementStyle[styleProperty] = "";
  } else if (
    typeof styleValue != "number" ||
    NON_DIMENSIONAL_CSS_REGEX.test(styleProperty)
  ) {
    elementStyle[styleProperty] = styleValue;
  } else {
    elementStyle[styleProperty] = styleValue + "px";
  }
}
processRerenderQueue._rerenderCount = 0;
var CAPTURE_SUFFIX_REGEX = /(PointerCapture)$|Capture$/i;
var ATTACH_EVENT_COUNTER = 0;
function setElementProperty(
  element,
  propertyName,
  newValue,
  oldValue,
  namespace
) {
  let isCapture;
  e: if (propertyName == "style") {
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
  } else if (propertyName[0] == "o" && propertyName[1] == "n") {
    isCapture =
      propertyName !=
      (propertyName = propertyName.replace(CAPTURE_SUFFIX_REGEX, "$1"));
    if (
      propertyName.toLowerCase() in element ||
      propertyName == "onFocusOut" ||
      propertyName == "onFocusIn"
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
        newValue._attached = ATTACH_EVENT_COUNTER;
        element.addEventListener(
          propertyName,
          isCapture ? eventHandlerCapture : eventHandler,
          isCapture
        );
      }
    } else {
      element.removeEventListener(
        propertyName,
        isCapture ? eventHandlerCapture : eventHandler,
        isCapture
      );
    }
  } else {
    if (namespace == SVG_NS) {
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
      } catch (styleLabel) {}
    }
    if (!(typeof newValue == "function")) {
      if (newValue == null || (newValue === false && propertyName[4] != "-")) {
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
        event._dispatched = ATTACH_EVENT_COUNTER++;
      } else if (event._dispatched < listener._attached) {
        return;
      }
      return listener(
        defaultOptions.event ? defaultOptions.event(event) : event
      );
    }
  };
}
var eventHandler = createEventHandlerProxy(false);
var eventHandlerCapture = createEventHandlerProxy(true);
function diffNodes(
  parentVNode,
  newVNode,
  oldVNode,
  globalContext,
  namespaceURI,
  childNodeArray,
  componentCallbackQueue,
  domNode,
  isHydrating,
  postRenderQueue
) {
  let diffHook;
  let componentType = newVNode.type;
  if (newVNode.constructor !== UNDEFINED_VALUE) {
    return null;
  }
  if (oldVNode._flags & SUSPENDED_MODE) {
    isHydrating = !!(oldVNode._flags & HYDRATE_MODE);
    childNodeArray = [(domNode = newVNode._dom = oldVNode._dom)];
  }
  if ((diffHook = defaultOptions._diff)) {
    diffHook(newVNode);
  }
  e: if (typeof componentType == "function") {
    try {
      let componentInstance,
        isMounting,
        prevProps,
        prevState,
        snapshot,
        previousError;
      let newProps = newVNode.props;
      const isClassComponent =
        "prototype" in componentType && componentType.prototype.render;
      diffHook = componentType.contextType;
      let contextProviderInstance = diffHook && globalContext[diffHook._id];
      let contextValue = diffHook
        ? contextProviderInstance
          ? contextProviderInstance.props.value
          : diffHook._defaultValue
        : globalContext;
      if (oldVNode._component) {
        componentInstance = newVNode._component = oldVNode._component;
        previousError = componentInstance._processingException =
          componentInstance._pendingError;
      } else {
        if (isClassComponent) {
          newVNode._component = componentInstance = new componentType(
            newProps,
            contextValue
          );
        } else {
          newVNode._component = componentInstance = new ComponentBase(
            newProps,
            contextValue
          );
          componentInstance.constructor = componentType;
          componentInstance.render = renderElement;
        }
        if (contextProviderInstance) {
          contextProviderInstance.sub(componentInstance);
        }
        componentInstance.props = newProps;
        if (!componentInstance.state) {
          componentInstance.state = {};
        }
        componentInstance.context = contextValue;
        componentInstance._globalContext = globalContext;
        isMounting = componentInstance._dirty = true;
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
            newProps,
            componentInstance._nextState
          )
        );
      }
      prevProps = componentInstance.props;
      prevState = componentInstance.state;
      componentInstance._vnode = newVNode;
      if (isMounting) {
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
          newProps !== prevProps &&
          componentInstance.componentWillReceiveProps != null
        ) {
          componentInstance.componentWillReceiveProps(newProps, contextValue);
        }
        if (
          !componentInstance._force &&
          ((componentInstance.shouldComponentUpdate != null &&
            false ===
              componentInstance.shouldComponentUpdate(
                newProps,
                componentInstance._nextState,
                contextValue
              )) ||
            newVNode._original == oldVNode._original)
        ) {
          if (newVNode._original != oldVNode._original) {
            componentInstance.props = newProps;
            componentInstance.state = componentInstance._nextState;
            componentInstance._dirty = false;
          }
          newVNode._dom = oldVNode._dom;
          newVNode._children = oldVNode._children;
          newVNode._children.some((child) => {
            if (child) {
              child._parent = newVNode;
            }
          });
          for (
            let loopIndex = 0;
            loopIndex < componentInstance._stateCallbacks.length;
            loopIndex++
          ) {
            componentInstance._renderCallbacks.push(
              componentInstance._stateCallbacks[loopIndex]
            );
          }
          componentInstance._stateCallbacks = [];
          if (componentInstance._renderCallbacks.length) {
            componentCallbackQueue.push(componentInstance);
          }
          break e;
        }
        if (componentInstance.componentWillUpdate != null) {
          componentInstance.componentWillUpdate(
            newProps,
            componentInstance._nextState,
            contextValue
          );
        }
        if (isClassComponent && componentInstance.componentDidUpdate != null) {
          componentInstance._renderCallbacks.push(() => {
            componentInstance.componentDidUpdate(
              prevProps,
              prevState,
              snapshot
            );
          });
        }
      }
      componentInstance.context = contextValue;
      componentInstance.props = newProps;
      componentInstance._parentDom = parentVNode;
      componentInstance._force = false;
      let renderHook = defaultOptions._render;
      let renderAttempts = 0;
      if (isClassComponent) {
        componentInstance.state = componentInstance._nextState;
        componentInstance._dirty = false;
        if (renderHook) {
          renderHook(newVNode);
        }
        diffHook = componentInstance.render(
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
            renderHook(newVNode);
          }
          diffHook = componentInstance.render(
            componentInstance.props,
            componentInstance.state,
            componentInstance.context
          );
          componentInstance.state = componentInstance._nextState;
        } while (componentInstance._dirty && ++renderAttempts < 25);
      }
      componentInstance.state = componentInstance._nextState;
      if (componentInstance.getChildContext != null) {
        globalContext = mergeObjects(
          mergeObjects({}, globalContext),
          componentInstance.getChildContext()
        );
      }
      if (
        isClassComponent &&
        !isMounting &&
        componentInstance.getSnapshotBeforeUpdate != null
      ) {
        snapshot = componentInstance.getSnapshotBeforeUpdate(
          prevProps,
          prevState
        );
      }
      let childrenToDiff =
        diffHook != null &&
        diffHook.type === getChildrenFromNode &&
        diffHook.key == null
          ? diffHook.props.children
          : diffHook;
      domNode = diffChildrenNodes(
        parentVNode,
        isArrayCheck(childrenToDiff) ? childrenToDiff : [childrenToDiff],
        newVNode,
        oldVNode,
        globalContext,
        namespaceURI,
        childNodeArray,
        componentCallbackQueue,
        domNode,
        isHydrating,
        postRenderQueue
      );
      componentInstance.base = newVNode._dom;
      newVNode._flags &= RESET_FLAGS_MASK;
      if (componentInstance._renderCallbacks.length) {
        componentCallbackQueue.push(componentInstance);
      }
      if (previousError) {
        componentInstance._pendingError =
          componentInstance._processingException = null;
      }
    } catch (componentUpdateSkip) {
      newVNode._original = null;
      if (isHydrating || childNodeArray != null) {
        if (componentUpdateSkip.then) {
          for (
            newVNode._flags |= isHydrating
              ? HYDRATE_MODE | SUSPENDED_MODE
              : SUSPENDED_MODE;
            domNode && domNode.nodeType == 8 && domNode.nextSibling;

          ) {
            domNode = domNode.nextSibling;
          }
          childNodeArray[childNodeArray.indexOf(domNode)] = null;
          newVNode._dom = domNode;
        } else {
          for (let e = childNodeArray.length; e--; ) {
            removeDomNode(childNodeArray[e]);
          }
        }
      } else {
        newVNode._dom = oldVNode._dom;
        newVNode._children = oldVNode._children;
      }
      defaultOptions._catchError(componentUpdateSkip, newVNode, oldVNode);
    }
  } else if (
    childNodeArray == null &&
    newVNode._original == oldVNode._original
  ) {
    newVNode._children = oldVNode._children;
    newVNode._dom = oldVNode._dom;
  } else {
    domNode = newVNode._dom = patchElementTree(
      oldVNode._dom,
      newVNode,
      oldVNode,
      globalContext,
      namespaceURI,
      childNodeArray,
      componentCallbackQueue,
      isHydrating,
      postRenderQueue
    );
  }
  if ((diffHook = defaultOptions.diffed)) {
    diffHook(newVNode);
  }
  if (newVNode._flags & SUSPENDED_MODE) {
    return undefined;
  } else {
    return domNode;
  }
}
function commitRootFiberTree(rootArray, fiberNode, refArray) {
  for (let index = 0; index < refArray.length; index++) {
    setRef(refArray[index], refArray[++index], refArray[++index]);
  }
  if (defaultOptions._commit) {
    defaultOptions._commit(fiberNode, rootArray);
  }
  rootArray.some((component) => {
    try {
      rootArray = component._renderCallbacks;
      component._renderCallbacks = [];
      rootArray.some((callback) => {
        callback.call(component);
      });
    } catch (renderErr) {
      defaultOptions._catchError(renderErr, component._vnode);
    }
  });
}
function patchElementTree(
  domElement,
  newVNode,
  oldVNode,
  parentVNode,
  namespace,
  existingChildNodes,
  childIndex,
  isHydrating,
  nextSibling
) {
  let loopIndex,
    newDangerouslySetInnerHTML,
    oldDangerouslySetInnerHTML,
    newChildren,
    attributeOrNode,
    newValue,
    newChecked;
  let oldProps = oldVNode.props;
  let newProps = newVNode.props;
  let newType = newVNode.type;
  if (newType == "svg") {
    namespace = SVG_NS;
  } else if (newType == "math") {
    namespace = MATH_NS;
  } else if (!namespace) {
    namespace = XHTML_NS;
  }
  if (existingChildNodes != null) {
    for (loopIndex = 0; loopIndex < existingChildNodes.length; loopIndex++) {
      attributeOrNode = existingChildNodes[loopIndex];
      if (
        attributeOrNode &&
        "setAttribute" in attributeOrNode == !!newType &&
        (newType
          ? attributeOrNode.localName == newType
          : attributeOrNode.nodeType == 3)
      ) {
        domElement = attributeOrNode;
        existingChildNodes[loopIndex] = null;
        break;
      }
    }
  }
  if (domElement == null) {
    if (newType == null) {
      return document.createTextNode(newProps);
    }
    domElement = document.createElementNS(
      namespace,
      newType,
      newProps.is && newProps
    );
    if (isHydrating) {
      if (defaultOptions._hydrationMismatch) {
        defaultOptions._hydrationMismatch(newVNode, existingChildNodes);
      }
      isHydrating = false;
    }
    existingChildNodes = null;
  }
  if (newType === null) {
    if (
      !(oldProps === newProps || (isHydrating && domElement.data === newProps))
    ) {
      domElement.data = newProps;
    }
  } else {
    existingChildNodes =
      existingChildNodes && arraySlice.call(domElement.childNodes);
    oldProps = oldVNode.props || EMPTY_OBJECT;
    if (!isHydrating && existingChildNodes != null) {
      for (
        oldProps = {}, loopIndex = 0;
        loopIndex < domElement.attributes.length;
        loopIndex++
      ) {
        attributeOrNode = domElement.attributes[loopIndex];
        oldProps[attributeOrNode.name] = attributeOrNode.value;
      }
    }
    for (loopIndex in oldProps) {
      attributeOrNode = oldProps[loopIndex];
      if (loopIndex == "children") {
      } else if (loopIndex == "dangerouslySetInnerHTML") {
        oldDangerouslySetInnerHTML = attributeOrNode;
      } else if (!(loopIndex in newProps)) {
        if (
          (loopIndex == "value" && "defaultValue" in newProps) ||
          (loopIndex == "checked" && "defaultChecked" in newProps)
        ) {
          continue;
        }
        setElementProperty(
          domElement,
          loopIndex,
          null,
          attributeOrNode,
          namespace
        );
      }
    }
    for (loopIndex in newProps) {
      attributeOrNode = newProps[loopIndex];
      if (loopIndex == "children") {
        newChildren = attributeOrNode;
      } else if (loopIndex == "dangerouslySetInnerHTML") {
        newDangerouslySetInnerHTML = attributeOrNode;
      } else if (loopIndex == "value") {
        newValue = attributeOrNode;
      } else if (loopIndex == "checked") {
        newChecked = attributeOrNode;
      } else if (
        !(
          (isHydrating && typeof attributeOrNode != "function") ||
          oldProps[loopIndex] === attributeOrNode
        )
      ) {
        setElementProperty(
          domElement,
          loopIndex,
          attributeOrNode,
          oldProps[loopIndex],
          namespace
        );
      }
    }
    if (newDangerouslySetInnerHTML) {
      if (
        !(
          isHydrating ||
          (oldDangerouslySetInnerHTML &&
            (newDangerouslySetInnerHTML.__html ===
              oldDangerouslySetInnerHTML.__html ||
              newDangerouslySetInnerHTML.__html === domElement.innerHTML))
        )
      ) {
        domElement.innerHTML = newDangerouslySetInnerHTML.__html;
      }
      newVNode._children = [];
    } else {
      if (oldDangerouslySetInnerHTML) {
        domElement.innerHTML = "";
      }
      diffChildrenNodes(
        newVNode.type === "template" ? domElement.content : domElement,
        isArrayCheck(newChildren) ? newChildren : [newChildren],
        newVNode,
        oldVNode,
        parentVNode,
        newType == "foreignObject" ? XHTML_NS : namespace,
        existingChildNodes,
        childIndex,
        existingChildNodes
          ? existingChildNodes[0]
          : oldVNode._children && findNextDomSibling(oldVNode, 0),
        isHydrating,
        nextSibling
      );
      if (existingChildNodes != null) {
        for (loopIndex = existingChildNodes.length; loopIndex--; ) {
          removeDomNode(existingChildNodes[loopIndex]);
        }
      }
    }
    if (!isHydrating) {
      loopIndex = "value";
      if (newType == "progress" && newValue == null) {
        domElement.removeAttribute("value");
      } else if (
        newValue !== UNDEFINED_VALUE &&
        (newValue !== domElement[loopIndex] ||
          (newType == "progress" && !newValue) ||
          (newType == "option" && newValue !== oldProps[loopIndex]))
      ) {
        setElementProperty(
          domElement,
          loopIndex,
          newValue,
          oldProps[loopIndex],
          namespace
        );
      }
      loopIndex = "checked";
      if (
        newChecked !== UNDEFINED_VALUE &&
        newChecked !== domElement[loopIndex]
      ) {
        setElementProperty(
          domElement,
          loopIndex,
          newChecked,
          oldProps[loopIndex],
          namespace
        );
      }
    }
  }
  return domElement;
}
function setRef(ref, value, errorContext) {
  try {
    if (typeof ref == "function") {
      let hasUnmountMethod = typeof ref._unmount == "function";
      if (hasUnmountMethod) {
        ref._unmount();
      }
      if (!(hasUnmountMethod && value == null)) {
        ref._unmount = ref(value);
      }
    } else {
      ref.current = value;
    }
  } catch (caughtError) {
    defaultOptions._catchError(caughtError, errorContext);
  }
}
function unmountComponent(component, errorContext, skipDomRemoval) {
  let tempRefComponentChildren;
  if (defaultOptions.unmount) {
    defaultOptions.unmount(component);
  }
  if ((tempRefComponentChildren = component.ref)) {
    if (
      !(
        tempRefComponentChildren.current &&
        tempRefComponentChildren.current !== component._dom
      )
    ) {
      setRef(tempRefComponentChildren, null, errorContext);
    }
  }
  if ((tempRefComponentChildren = component._component) != null) {
    if (tempRefComponentChildren.componentWillUnmount) {
      try {
        tempRefComponentChildren.componentWillUnmount();
      } catch (unmountError) {
        defaultOptions._catchError(unmountError, errorContext);
      }
    }
    tempRefComponentChildren.base = tempRefComponentChildren._parentDom = null;
  }
  if ((tempRefComponentChildren = component._children)) {
    for (
      let childIndex = 0;
      childIndex < tempRefComponentChildren.length;
      childIndex++
    ) {
      if (tempRefComponentChildren[childIndex]) {
        unmountComponent(
          tempRefComponentChildren[childIndex],
          errorContext,
          skipDomRemoval || typeof component.type != "function"
        );
      }
    }
  }
  if (!skipDomRemoval) {
    removeDomNode(component._dom);
  }
  component._component = component._parent = component._dom = UNDEFINED_VALUE;
}
function renderElement(element, unused, context) {
  return this.constructor(element, context);
}
function renderVirtualNode(element, container, hydrateOption) {
  if (container == document) {
    container = document.documentElement;
  }
  if (defaultOptions._root) {
    defaultOptions._root(element, container);
  }
  let isHydrateFunction = typeof hydrateOption == "function";
  let existingChildren = isHydrateFunction
    ? null
    : (hydrateOption && hydrateOption._children) || container._children;
  let fiberArray = [];
  let refArray = [];
  diffNodes(
    container,
    (element = ((!isHydrateFunction && hydrateOption) || container)._children =
      createVirtualElement(getChildrenFromNode, null, [element])),
    existingChildren || EMPTY_OBJECT,
    EMPTY_OBJECT,
    container.namespaceURI,
    !isHydrateFunction && hydrateOption
      ? [hydrateOption]
      : existingChildren
        ? null
        : container.firstChild
          ? arraySlice.call(container.childNodes)
          : null,
    fiberArray,
    !isHydrateFunction && hydrateOption
      ? hydrateOption
      : existingChildren
        ? existingChildren._dom
        : container.firstChild,
    isHydrateFunction,
    refArray
  );
  commitRootFiberTree(fiberArray, element, refArray);
}
function hydrateRoot(container, root) {
  renderVirtualNode(container, root, hydrateRoot);
}
function cloneVirtualElement(originalElement, overriddenProps, children) {
  let newKey, newRef, propKey, defaultProps;
  let mergedProps = mergeObjects({}, originalElement.props);
  if (originalElement.type && originalElement.type.defaultProps) {
    defaultProps = originalElement.type.defaultProps;
  }
  for (propKey in overriddenProps)
    if (propKey == "key") {
      newKey = overriddenProps[propKey];
    } else if (propKey == "ref") {
      newRef = overriddenProps[propKey];
    } else if (
      overriddenProps[propKey] === UNDEFINED_VALUE &&
      defaultProps !== UNDEFINED_VALUE
    ) {
      mergedProps[propKey] = defaultProps[propKey];
    } else {
      mergedProps[propKey] = overriddenProps[propKey];
    }
  if (arguments.length > 2) {
    mergedProps.children =
      arguments.length > 3 ? arraySlice.call(arguments, 2) : children;
  }
  return createVirtualNode(
    originalElement.type,
    mergedProps,
    newKey || originalElement.key,
    newRef || originalElement.ref,
    null
  );
}
var childVNode = 0;
function createContextObject(defaultValue) {
  function Provider(element) {
    if (!this.getChildContext) {
      let subscribers = new Set();
      let contextObject = {};
      contextObject[Provider._id] = this;
      this.getChildContext = () => contextObject;
      this.componentWillUnmount = () => {
        subscribers = null;
      };
      this.shouldComponentUpdate = function (nextProps) {
        if (this.props.value !== nextProps.value) {
          subscribers.forEach((component) => {
            component._force = true;
            scheduleRender(component);
          });
        }
      };
      this.sub = (component) => {
        subscribers.add(component);
        let originalUnmount = component.componentWillUnmount;
        component.componentWillUnmount = () => {
          if (subscribers) {
            subscribers.delete(component);
          }
          if (originalUnmount) {
            originalUnmount.call(component);
          }
        };
      };
    }
    return element.children;
  }
  Provider._id = "__cC" + childVNode++;
  Provider._defaultValue = defaultValue;
  Provider.Consumer = (parent, selector) => parent.children(selector);
  Provider.Provider =
    Provider._contextRef =
    Provider.Consumer.contextType =
      Provider;
  return Provider;
}
export {
  ComponentBase as Component,
  getChildrenFromNode as Fragment,
  cloneVirtualElement as cloneElement,
  createContextObject as createContext,
  createVirtualElement as createElement,
  createReferenceObject as createRef,
  createVirtualElement as h,
  hydrateRoot as hydrate,
  isValidComponentInstance as isValidElement,
  defaultOptions as options,
  renderVirtualNode as render,
  flattenNonBooleanElements as toChildArray
};
