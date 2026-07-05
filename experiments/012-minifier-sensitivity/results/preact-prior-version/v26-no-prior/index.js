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
var isArrayFn = Array.isArray;
function mergeObjects(target, source) {
  for (let nextState in source) target[nextState] = source[nextState];
  return target;
}
function removeDomNode(node) {
  if (node && node.parentNode) {
    node.parentNode.removeChild(node);
  }
}
var sliceArray = EMPTY_ARRAY.slice;
function propagateError(error, component, sourceComponent, errorInfo) {
  let handlerComponent, ComponentConstructor, needsUpdate;
  for (; (component = component._parent); ) {
    if (
      (handlerComponent = component._component) &&
      !handlerComponent._processingException
    ) {
      try {
        ComponentConstructor = handlerComponent.constructor;
        if (
          ComponentConstructor &&
          ComponentConstructor.getDerivedStateFromError != null
        ) {
          handlerComponent.setState(
            ComponentConstructor.getDerivedStateFromError(error)
          );
          needsUpdate = handlerComponent._dirty;
        }
        if (handlerComponent.componentDidCatch != null) {
          handlerComponent.componentDidCatch(error, errorInfo || {});
          needsUpdate = handlerComponent._dirty;
        }
        if (needsUpdate) {
          return (handlerComponent._pendingError = handlerComponent);
        }
      } catch (caughtError) {
        error = caughtError;
      }
    }
  }
  throw error;
}
var defaultOptions = {
  _catchError: propagateError
};
var defaultOptionsRef = defaultOptions;
var vnodeIdCounter = 0;
function createVirtualElement(nodeType, props, children) {
  let key, ref, propKey;
  let mergedProps = {};
  for (propKey in props)
    if (propKey == "key") {
      key = props[propKey];
    } else if (propKey == "ref") {
      ref = props[propKey];
    } else {
      mergedProps[propKey] = props[propKey];
    }
  if (arguments.length > 2) {
    mergedProps.children =
      arguments.length > 3 ? sliceArray.call(arguments, 2) : children;
  }
  if (typeof nodeType == "function" && nodeType.defaultProps != null) {
    for (propKey in nodeType.defaultProps)
      if (mergedProps[propKey] === UNDEFINED_VALUE) {
        mergedProps[propKey] = nodeType.defaultProps[propKey];
      }
  }
  return createVirtualNode(nodeType, mergedProps, key, ref, null);
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
    _component: null,
    constructor: UNDEFINED_VALUE,
    _original: originalId == null ? ++vnodeIdCounter : originalId,
    _index: -1,
    _flags: 0
  };
  if (originalId == null && defaultOptionsRef.vnode != null) {
    defaultOptionsRef.vnode(vnode);
  }
  return vnode;
}
function createReference() {
  return {
    current: null
  };
}
function extractFragmentChildren(node) {
  return node.children;
}
var isValidComponent = (value) =>
  value != null && value.constructor == UNDEFINED_VALUE;
function ComponentBase(props, context) {
  this.props = props;
  this.context = context;
}
function findNextDomSibling(node, startIndex) {
  if (startIndex == null) {
    if (node._parent) {
      return findNextDomSibling(node._parent, node._index + 1);
    } else {
      return null;
    }
  }
  let child;
  for (; startIndex < node._children.length; startIndex++) {
    child = node._children[startIndex];
    if (child != null && child._dom != null) {
      return child._dom;
    }
  }
  if (typeof node.type == "function") {
    return findNextDomSibling(node);
  } else {
    return null;
  }
}
function renderComponentInstance(componentInstance) {
  let vnode = componentInstance._vnode;
  let domNode = vnode._dom;
  let pendingComponents = [];
  let referenceTriples = [];
  if (componentInstance._parentDom) {
    const mergedVnode = mergeObjects({}, vnode);
    mergedVnode._original = vnode._original + 1;
    if (defaultOptionsRef.vnode) {
      defaultOptionsRef.vnode(mergedVnode);
    }
    diffNodes(
      componentInstance._parentDom,
      mergedVnode,
      vnode,
      componentInstance._globalContext,
      componentInstance._parentDom.namespaceURI,
      vnode._flags & HYDRATE_MODE ? [domNode] : null,
      pendingComponents,
      domNode == null ? findNextDomSibling(vnode) : domNode,
      !!(vnode._flags & HYDRATE_MODE),
      referenceTriples
    );
    mergedVnode._original = vnode._original;
    mergedVnode._parent._children[mergedVnode._index] = mergedVnode;
    commitRootChanges(pendingComponents, mergedVnode, referenceTriples);
    if (mergedVnode._dom != domNode) {
      propagateDomPointerToParent(mergedVnode);
    }
  }
}
function propagateDomPointerToParent(parent) {
  if ((parent = parent._parent) != null && parent._component != null) {
    parent._dom = parent._component.base = null;
    for (let index = 0; index < parent._children.length; index++) {
      let child = parent._children[index];
      if (child != null && child._dom != null) {
        parent._dom = parent._component.base = child._dom;
        break;
      }
    }
    return propagateDomPointerToParent(parent);
  }
}
ComponentBase.prototype.setState = function (stateOrUpdater, stateCallback) {
  let nextState;
  if (this._nextState != null && this._nextState !== this.state) {
    nextState = this._nextState;
  } else {
    nextState = this._nextState = mergeObjects({}, this.state);
  }
  if (typeof stateOrUpdater == "function") {
    stateOrUpdater = stateOrUpdater(mergeObjects({}, nextState), this.props);
  }
  if (stateOrUpdater) {
    mergeObjects(nextState, stateOrUpdater);
  }
  if (stateOrUpdater != null && this._vnode) {
    if (stateCallback) {
      this._stateCallbacks.push(stateCallback);
    }
    scheduleComponentRender(this);
  }
};
ComponentBase.prototype.forceUpdate = function (callback) {
  if (this._vnode) {
    this._force = true;
    if (callback) {
      this._renderCallbacks.push(callback);
    }
    scheduleComponentRender(this);
  }
};
ComponentBase.prototype.render = extractFragmentChildren;
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
    previousDebounceFn !== defaultOptionsRef.debounceRendering
  ) {
    (
      (previousDebounceFn = defaultOptionsRef.debounceRendering) || scheduleTask
    )(handleRerenderQueue);
  }
}
var compareDepth = (nodeA, nodeB) => nodeA._vnode._depth - nodeB._vnode._depth;
function handleRerenderQueue() {
  let component;
  let queueLength = 1;
  for (; pendingRerenderQueue.length; ) {
    if (pendingRerenderQueue.length > queueLength) {
      pendingRerenderQueue.sort(compareDepth);
    }
    component = pendingRerenderQueue.shift();
    queueLength = pendingRerenderQueue.length;
    if (component._dirty) {
      renderComponentInstance(component);
    }
  }
  handleRerenderQueue._rerenderCount = 0;
}
function diffChildrenVnodes(
  parentDom,
  newChildren,
  parentVnode,
  oldParentVnode,
  namespace,
  flags,
  component,
  context,
  newChildrenArray,
  nextSiblingDom,
  refUpdates
) {
  let index, oldChild, newChild, childDom, firstChildDom;
  let oldChildren = (oldParentVnode && oldParentVnode._children) || EMPTY_ARRAY;
  let childCount = newChildren.length;
  for (
    newChildrenArray = buildNewChildrenArray(
      parentVnode,
      newChildren,
      oldChildren,
      newChildrenArray,
      childCount
    ),
      index = 0;
    index < childCount;
    index++
  ) {
    newChild = parentVnode._children[index];
    if (newChild == null) {
      continue;
    }
    if (-1 === newChild._index) {
      oldChild = EMPTY_OBJECT;
    } else {
      oldChild = oldChildren[newChild._index] || EMPTY_OBJECT;
    }
    newChild._index = index;
    let t = diffNodes(
      parentDom,
      newChild,
      oldChild,
      namespace,
      flags,
      component,
      context,
      newChildrenArray,
      nextSiblingDom,
      refUpdates
    );
    childDom = newChild._dom;
    if (newChild.ref && oldChild.ref != newChild.ref) {
      if (oldChild.ref) {
        applyReference(oldChild.ref, null, newChild);
      }
      refUpdates.push(newChild.ref, newChild._component || childDom, newChild);
    }
    if (firstChildDom == null && childDom != null) {
      firstChildDom = childDom;
    }
    if (
      newChild._flags & INSERT_VNODE_FLAG ||
      oldChild._children === newChild._children
    ) {
      newChildrenArray = insertComponent(newChild, newChildrenArray, parentDom);
    } else if (typeof newChild.type == "function" && t !== UNDEFINED_VALUE) {
      newChildrenArray = t;
    } else if (childDom) {
      newChildrenArray = childDom.nextSibling;
    }
    newChild._flags &= ~(INSERT_VNODE_FLAG | MATCHED_FLAG);
  }
  parentVnode._dom = firstChildDom;
  return newChildrenArray;
}
function buildNewChildrenArray(
  parentVNode,
  newChildrenArray,
  oldChildrenArray,
  parentDomNode,
  newChildrenCount
) {
  let newChildIndex, childNode, matchedOldChild;
  let oldChildrenLength = oldChildrenArray.length;
  let unmatchedOldChildrenCount = oldChildrenLength;
  let indexDelta = 0;
  for (
    parentVNode._children = new Array(newChildrenCount), newChildIndex = 0;
    newChildIndex < newChildrenCount;
    newChildIndex++
  ) {
    childNode = newChildrenArray[newChildIndex];
    if (
      childNode == null ||
      typeof childNode == "boolean" ||
      typeof childNode == "function"
    ) {
      parentVNode._children[newChildIndex] = null;
      continue;
    }
    if (
      typeof childNode == "string" ||
      typeof childNode == "number" ||
      typeof childNode == "bigint" ||
      childNode.constructor == String
    ) {
      childNode = parentVNode._children[newChildIndex] = createVirtualNode(
        null,
        childNode,
        null,
        null,
        null
      );
    } else if (isArrayFn(childNode)) {
      childNode = parentVNode._children[newChildIndex] = createVirtualNode(
        extractFragmentChildren,
        {
          children: childNode
        },
        null,
        null,
        null
      );
    } else if (
      childNode.constructor === UNDEFINED_VALUE &&
      childNode._depth > 0
    ) {
      childNode = parentVNode._children[newChildIndex] = createVirtualNode(
        childNode.type,
        childNode.props,
        childNode.key,
        childNode.ref ? childNode.ref : null,
        childNode._original
      );
    } else {
      childNode = parentVNode._children[newChildIndex] = childNode;
    }
    const o = newChildIndex + indexDelta;
    childNode._parent = parentVNode;
    childNode._depth = parentVNode._depth + 1;
    const r = (childNode._index = findMatchingIndexInArray(
      childNode,
      oldChildrenArray,
      o,
      unmatchedOldChildrenCount
    ));
    matchedOldChild = null;
    if (-1 !== r) {
      matchedOldChild = oldChildrenArray[r];
      unmatchedOldChildrenCount--;
      if (matchedOldChild) {
        matchedOldChild._flags |= MATCHED_FLAG;
      }
    }
    if (matchedOldChild == null || matchedOldChild._original === null) {
      if (-1 == r) {
        indexDelta--;
      }
      if (typeof childNode.type != "function") {
        childNode._flags |= INSERT_VNODE_FLAG;
      }
    } else if (r != o) {
      if (r == o - 1) {
        indexDelta--;
      } else if (r == o + 1) {
        indexDelta++;
      } else {
        if (r > o) {
          indexDelta--;
        } else {
          indexDelta++;
        }
        childNode._flags |= INSERT_VNODE_FLAG;
      }
    }
  }
  if (unmatchedOldChildrenCount) {
    for (
      newChildIndex = 0;
      newChildIndex < oldChildrenLength;
      newChildIndex++
    ) {
      matchedOldChild = oldChildrenArray[newChildIndex];
      if (
        matchedOldChild != null &&
        (matchedOldChild._flags & MATCHED_FLAG) == 0
      ) {
        if (matchedOldChild._dom == parentDomNode) {
          parentDomNode = findNextDomSibling(matchedOldChild);
        }
        unmountComponent(matchedOldChild, matchedOldChild);
      }
    }
  }
  return parentDomNode;
}
function insertComponent(component, nextSibling, parentDom) {
  if (typeof component.type == "function") {
    let children = component._children;
    for (let index = 0; children && index < children.length; index++) {
      if (children[index]) {
        children[index]._parent = component;
        nextSibling = insertComponent(children[index], nextSibling, parentDom);
      }
    }
    return nextSibling;
  }
  if (component._dom != nextSibling) {
    if (nextSibling && component.type && !parentDom.contains(nextSibling)) {
      nextSibling = findNextDomSibling(component);
    }
    parentDom.insertBefore(component._dom, nextSibling || null);
    nextSibling = component._dom;
  }
  do {
    nextSibling = nextSibling && nextSibling.nextSibling;
  } while (nextSibling != null && nextSibling.nodeType == 8);
  return nextSibling;
}
function flattenNonBooleanValues(node, accumulator) {
  accumulator = accumulator || [];
  if (!(node == null || typeof node == "boolean")) {
    if (isArrayFn(node)) {
      node.some((element) => {
        flattenNonBooleanValues(element, accumulator);
      });
    } else {
      accumulator.push(node);
    }
  }
  return accumulator;
}
function findMatchingIndexInArray(node, nodes, startIndex, maxDistance) {
  const key = node.key;
  const type = node.type;
  let currentNode = nodes[startIndex];
  let shouldSearchNeighbors =
    maxDistance >
    (currentNode != null && (currentNode._flags & MATCHED_FLAG) == 0 ? 1 : 0);
  if (
    currentNode === null ||
    (currentNode &&
      key == currentNode.key &&
      type === currentNode.type &&
      (currentNode._flags & MATCHED_FLAG) == 0)
  ) {
    return startIndex;
  }
  if (shouldSearchNeighbors) {
    let e = startIndex - 1;
    let o = startIndex + 1;
    for (; e >= 0 || o < nodes.length; ) {
      if (e >= 0) {
        currentNode = nodes[e];
        if (
          currentNode &&
          (currentNode._flags & MATCHED_FLAG) == 0 &&
          key == currentNode.key &&
          type === currentNode.type
        ) {
          return e;
        }
        e--;
      }
      if (o < nodes.length) {
        currentNode = nodes[o];
        if (
          currentNode &&
          (currentNode._flags & MATCHED_FLAG) == 0 &&
          key == currentNode.key &&
          type === currentNode.type
        ) {
          return o;
        }
        o++;
      }
    }
  }
  return -1;
}
function applyStyle(styleObj, propertyName, value) {
  if (propertyName[0] == "-") {
    styleObj.setProperty(propertyName, value == null ? "" : value);
  } else if (value == null) {
    styleObj[propertyName] = "";
  } else if (
    typeof value != "number" ||
    NON_DIMENSIONAL_CSS_REGEX.test(propertyName)
  ) {
    styleObj[propertyName] = value;
  } else {
    styleObj[propertyName] = value + "px";
  }
}
handleRerenderQueue._rerenderCount = 0;
var CAPTURE_SUFFIX_REGEX = /(PointerCapture)$|Capture$/i;
var eventAttachmentIndex = 0;
function setElementProperty(element, propertyName, value, oldValue, namespace) {
  let isCapture;
  e: if (propertyName == "style") {
    if (typeof value == "string") {
      element.style.cssText = value;
    } else {
      if (typeof oldValue == "string") {
        element.style.cssText = oldValue = "";
      }
      if (oldValue) {
        for (propertyName in oldValue)
          if (!(value && propertyName in value)) {
            applyStyle(element.style, propertyName, "");
          }
      }
      if (value) {
        for (propertyName in value)
          if (!(oldValue && value[propertyName] === oldValue[propertyName])) {
            applyStyle(element.style, propertyName, value[propertyName]);
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
    element._listeners[propertyName + isCapture] = value;
    if (value) {
      if (oldValue) {
        value._attached = oldValue._attached;
      } else {
        value._attached = eventAttachmentIndex;
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
        element[propertyName] = value == null ? "" : value;
        break e;
      } catch (propertySetBlock) {}
    }
    if (!(typeof value == "function")) {
      if (value == null || (value === false && propertyName[4] != "-")) {
        element.removeAttribute(propertyName);
      } else {
        element.setAttribute(
          propertyName,
          propertyName == "popover" && value == 1 ? "" : value
        );
      }
    }
  }
}
function createEventListenerProxy(eventSuffix) {
  return function (event) {
    if (this._listeners) {
      const listener = this._listeners[event.type + eventSuffix];
      if (event._dispatched == null) {
        event._dispatched = eventAttachmentIndex++;
      } else if (event._dispatched < listener._attached) {
        return;
      }
      return listener(
        defaultOptionsRef.event ? defaultOptionsRef.event(event) : event
      );
    }
  };
}
var eventHandler = createEventListenerProxy(false);
var eventHandlerCapture = createEventListenerProxy(true);
function diffNodes(
  parentDom,
  newVNode,
  oldVNode,
  globalContext,
  namespaceURI,
  suspendedNodes,
  renderQueue,
  domNode,
  isHydrating,
  childDiffContext
) {
  let placeholder;
  let componentType = newVNode.type;
  if (newVNode.constructor !== UNDEFINED_VALUE) {
    return null;
  }
  if (oldVNode._flags & SUSPENDED_MODE) {
    isHydrating = !!(oldVNode._flags & HYDRATE_MODE);
    suspendedNodes = [(domNode = newVNode._dom = oldVNode._dom)];
  }
  if ((placeholder = defaultOptionsRef._diff)) {
    placeholder(newVNode);
  }
  e: if (typeof componentType == "function") {
    try {
      let componentInstance,
        isDirty,
        componentProps,
        componentState,
        snapshotBeforeUpdate,
        pendingError;
      let vnodeProps = newVNode.props;
      const hasClassRender =
        "prototype" in componentType && componentType.prototype.render;
      placeholder = componentType.contextType;
      let contextProvider = placeholder && globalContext[placeholder._id];
      let contextValue = placeholder
        ? contextProvider
          ? contextProvider.props.value
          : placeholder._defaultValue
        : globalContext;
      if (oldVNode._component) {
        componentInstance = newVNode._component = oldVNode._component;
        pendingError = componentInstance._processingException =
          componentInstance._pendingError;
      } else {
        if (hasClassRender) {
          newVNode._component = componentInstance = new componentType(
            vnodeProps,
            contextValue
          );
        } else {
          newVNode._component = componentInstance = new ComponentBase(
            vnodeProps,
            contextValue
          );
          componentInstance.constructor = componentType;
          componentInstance.render = renderElement;
        }
        if (contextProvider) {
          contextProvider.sub(componentInstance);
        }
        componentInstance.props = vnodeProps;
        if (!componentInstance.state) {
          componentInstance.state = {};
        }
        componentInstance.context = contextValue;
        componentInstance._globalContext = globalContext;
        isDirty = componentInstance._dirty = true;
        componentInstance._renderCallbacks = [];
        componentInstance._stateCallbacks = [];
      }
      if (hasClassRender && componentInstance._nextState == null) {
        componentInstance._nextState = componentInstance.state;
      }
      if (hasClassRender && componentType.getDerivedStateFromProps != null) {
        if (componentInstance._nextState == componentInstance.state) {
          componentInstance._nextState = mergeObjects(
            {},
            componentInstance._nextState
          );
        }
        mergeObjects(
          componentInstance._nextState,
          componentType.getDerivedStateFromProps(
            vnodeProps,
            componentInstance._nextState
          )
        );
      }
      componentProps = componentInstance.props;
      componentState = componentInstance.state;
      componentInstance._vnode = newVNode;
      if (isDirty) {
        if (
          hasClassRender &&
          componentType.getDerivedStateFromProps == null &&
          componentInstance.componentWillMount != null
        ) {
          componentInstance.componentWillMount();
        }
        if (hasClassRender && componentInstance.componentDidMount != null) {
          componentInstance._renderCallbacks.push(
            componentInstance.componentDidMount
          );
        }
      } else {
        if (
          hasClassRender &&
          componentType.getDerivedStateFromProps == null &&
          vnodeProps !== componentProps &&
          componentInstance.componentWillReceiveProps != null
        ) {
          componentInstance.componentWillReceiveProps(vnodeProps, contextValue);
        }
        if (
          !componentInstance._force &&
          ((componentInstance.shouldComponentUpdate != null &&
            false ===
              componentInstance.shouldComponentUpdate(
                vnodeProps,
                componentInstance._nextState,
                contextValue
              )) ||
            newVNode._original == oldVNode._original)
        ) {
          if (newVNode._original != oldVNode._original) {
            componentInstance.props = vnodeProps;
            componentInstance.state = componentInstance._nextState;
            componentInstance._dirty = false;
          }
          newVNode._dom = oldVNode._dom;
          newVNode._children = oldVNode._children;
          newVNode._children.some((childNode) => {
            if (childNode) {
              childNode._parent = newVNode;
            }
          });
          for (let e = 0; e < componentInstance._stateCallbacks.length; e++) {
            componentInstance._renderCallbacks.push(
              componentInstance._stateCallbacks[e]
            );
          }
          componentInstance._stateCallbacks = [];
          if (componentInstance._renderCallbacks.length) {
            renderQueue.push(componentInstance);
          }
          break e;
        }
        if (componentInstance.componentWillUpdate != null) {
          componentInstance.componentWillUpdate(
            vnodeProps,
            componentInstance._nextState,
            contextValue
          );
        }
        if (hasClassRender && componentInstance.componentDidUpdate != null) {
          componentInstance._renderCallbacks.push(() => {
            componentInstance.componentDidUpdate(
              componentProps,
              componentState,
              snapshotBeforeUpdate
            );
          });
        }
      }
      componentInstance.context = contextValue;
      componentInstance.props = vnodeProps;
      componentInstance._parentDom = parentDom;
      componentInstance._force = false;
      let renderHook = defaultOptionsRef._render;
      let renderLoopCount = 0;
      if (hasClassRender) {
        componentInstance.state = componentInstance._nextState;
        componentInstance._dirty = false;
        if (renderHook) {
          renderHook(newVNode);
        }
        placeholder = componentInstance.render(
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
          placeholder = componentInstance.render(
            componentInstance.props,
            componentInstance.state,
            componentInstance.context
          );
          componentInstance.state = componentInstance._nextState;
        } while (componentInstance._dirty && ++renderLoopCount < 25);
      }
      componentInstance.state = componentInstance._nextState;
      if (componentInstance.getChildContext != null) {
        globalContext = mergeObjects(
          mergeObjects({}, globalContext),
          componentInstance.getChildContext()
        );
      }
      if (
        hasClassRender &&
        !isDirty &&
        componentInstance.getSnapshotBeforeUpdate != null
      ) {
        snapshotBeforeUpdate = componentInstance.getSnapshotBeforeUpdate(
          componentProps,
          componentState
        );
      }
      let renderedOutput =
        placeholder != null &&
        placeholder.type === extractFragmentChildren &&
        placeholder.key == null
          ? placeholder.props.children
          : placeholder;
      domNode = diffChildrenVnodes(
        parentDom,
        isArrayFn(renderedOutput) ? renderedOutput : [renderedOutput],
        newVNode,
        oldVNode,
        globalContext,
        namespaceURI,
        suspendedNodes,
        renderQueue,
        domNode,
        isHydrating,
        childDiffContext
      );
      componentInstance.base = newVNode._dom;
      newVNode._flags &= RESET_FLAGS_MASK;
      if (componentInstance._renderCallbacks.length) {
        renderQueue.push(componentInstance);
      }
      if (pendingError) {
        componentInstance._pendingError =
          componentInstance._processingException = null;
      }
    } catch (skipIndex) {
      newVNode._original = null;
      if (isHydrating || suspendedNodes != null) {
        if (skipIndex.then) {
          for (
            newVNode._flags |= isHydrating
              ? HYDRATE_MODE | SUSPENDED_MODE
              : SUSPENDED_MODE;
            domNode && domNode.nodeType == 8 && domNode.nextSibling;

          ) {
            domNode = domNode.nextSibling;
          }
          suspendedNodes[suspendedNodes.indexOf(domNode)] = null;
          newVNode._dom = domNode;
        } else {
          for (let e = suspendedNodes.length; e--; ) {
            removeDomNode(suspendedNodes[e]);
          }
        }
      } else {
        newVNode._dom = oldVNode._dom;
        newVNode._children = oldVNode._children;
      }
      defaultOptionsRef._catchError(skipIndex, newVNode, oldVNode);
    }
  } else if (
    suspendedNodes == null &&
    newVNode._original == oldVNode._original
  ) {
    newVNode._children = oldVNode._children;
    newVNode._dom = oldVNode._dom;
  } else {
    domNode = newVNode._dom = updateElementNodes(
      oldVNode._dom,
      newVNode,
      oldVNode,
      globalContext,
      namespaceURI,
      suspendedNodes,
      renderQueue,
      isHydrating,
      childDiffContext
    );
  }
  if ((placeholder = defaultOptionsRef.diffed)) {
    placeholder(newVNode);
  }
  if (newVNode._flags & SUSPENDED_MODE) {
    return undefined;
  } else {
    return domNode;
  }
}
function commitRootChanges(components, rootComponent, referenceTriples) {
  for (let e = 0; e < referenceTriples.length; e++) {
    applyReference(
      referenceTriples[e],
      referenceTriples[++e],
      referenceTriples[++e]
    );
  }
  if (defaultOptionsRef._commit) {
    defaultOptionsRef._commit(rootComponent, components);
  }
  components.some((componentInstance) => {
    try {
      components = componentInstance._renderCallbacks;
      componentInstance._renderCallbacks = [];
      components.some((callback) => {
        callback.call(componentInstance);
      });
    } catch (error) {
      defaultOptionsRef._catchError(error, componentInstance._vnode);
    }
  });
}
function updateElementNodes(
  existingDomNode,
  newVNode,
  oldVNode,
  parentDomNode,
  namespace,
  domChildren,
  nextSibling,
  isHydrating,
  isSvg
) {
  let idx,
    dangerouslySetInnerHTMLProp,
    oldDangerouslySetInnerHTML,
    newChildren,
    current,
    newValue,
    newChecked;
  let oldProps = oldVNode.props;
  let newProps = newVNode.props;
  let elementType = newVNode.type;
  if (elementType == "svg") {
    namespace = SVG_NS;
  } else if (elementType == "math") {
    namespace = MATH_NS;
  } else if (!namespace) {
    namespace = XHTML_NS;
  }
  if (domChildren != null) {
    for (idx = 0; idx < domChildren.length; idx++) {
      current = domChildren[idx];
      if (
        current &&
        "setAttribute" in current == !!elementType &&
        (elementType ? current.localName == elementType : current.nodeType == 3)
      ) {
        existingDomNode = current;
        domChildren[idx] = null;
        break;
      }
    }
  }
  if (existingDomNode == null) {
    if (elementType == null) {
      return document.createTextNode(newProps);
    }
    existingDomNode = document.createElementNS(
      namespace,
      elementType,
      newProps.is && newProps
    );
    if (isHydrating) {
      if (defaultOptionsRef._hydrationMismatch) {
        defaultOptionsRef._hydrationMismatch(newVNode, domChildren);
      }
      isHydrating = false;
    }
    domChildren = null;
  }
  if (elementType === null) {
    if (
      !(
        oldProps === newProps ||
        (isHydrating && existingDomNode.data === newProps)
      )
    ) {
      existingDomNode.data = newProps;
    }
  } else {
    domChildren = domChildren && sliceArray.call(existingDomNode.childNodes);
    oldProps = oldVNode.props || EMPTY_OBJECT;
    if (!isHydrating && domChildren != null) {
      for (
        oldProps = {}, idx = 0;
        idx < existingDomNode.attributes.length;
        idx++
      ) {
        current = existingDomNode.attributes[idx];
        oldProps[current.name] = current.value;
      }
    }
    for (idx in oldProps) {
      current = oldProps[idx];
      if (idx == "children") {
      } else if (idx == "dangerouslySetInnerHTML") {
        oldDangerouslySetInnerHTML = current;
      } else if (!(idx in newProps)) {
        if (
          (idx == "value" && "defaultValue" in newProps) ||
          (idx == "checked" && "defaultChecked" in newProps)
        ) {
          continue;
        }
        setElementProperty(existingDomNode, idx, null, current, namespace);
      }
    }
    for (idx in newProps) {
      current = newProps[idx];
      if (idx == "children") {
        newChildren = current;
      } else if (idx == "dangerouslySetInnerHTML") {
        dangerouslySetInnerHTMLProp = current;
      } else if (idx == "value") {
        newValue = current;
      } else if (idx == "checked") {
        newChecked = current;
      } else if (
        !(
          (isHydrating && typeof current != "function") ||
          oldProps[idx] === current
        )
      ) {
        setElementProperty(
          existingDomNode,
          idx,
          current,
          oldProps[idx],
          namespace
        );
      }
    }
    if (dangerouslySetInnerHTMLProp) {
      if (
        !(
          isHydrating ||
          (oldDangerouslySetInnerHTML &&
            (dangerouslySetInnerHTMLProp.__html ===
              oldDangerouslySetInnerHTML.__html ||
              dangerouslySetInnerHTMLProp.__html === existingDomNode.innerHTML))
        )
      ) {
        existingDomNode.innerHTML = dangerouslySetInnerHTMLProp.__html;
      }
      newVNode._children = [];
    } else {
      if (oldDangerouslySetInnerHTML) {
        existingDomNode.innerHTML = "";
      }
      diffChildrenVnodes(
        newVNode.type === "template"
          ? existingDomNode.content
          : existingDomNode,
        isArrayFn(newChildren) ? newChildren : [newChildren],
        newVNode,
        oldVNode,
        parentDomNode,
        elementType == "foreignObject" ? XHTML_NS : namespace,
        domChildren,
        nextSibling,
        domChildren
          ? domChildren[0]
          : oldVNode._children && findNextDomSibling(oldVNode, 0),
        isHydrating,
        isSvg
      );
      if (domChildren != null) {
        for (idx = domChildren.length; idx--; ) {
          removeDomNode(domChildren[idx]);
        }
      }
    }
    if (!isHydrating) {
      idx = "value";
      if (elementType == "progress" && newValue == null) {
        existingDomNode.removeAttribute("value");
      } else if (
        newValue !== UNDEFINED_VALUE &&
        (newValue !== existingDomNode[idx] ||
          (elementType == "progress" && !newValue) ||
          (elementType == "option" && newValue !== oldProps[idx]))
      ) {
        setElementProperty(
          existingDomNode,
          idx,
          newValue,
          oldProps[idx],
          namespace
        );
      }
      idx = "checked";
      if (
        newChecked !== UNDEFINED_VALUE &&
        newChecked !== existingDomNode[idx]
      ) {
        setElementProperty(
          existingDomNode,
          idx,
          newChecked,
          oldProps[idx],
          namespace
        );
      }
    }
  }
  return existingDomNode;
}
function applyReference(ref, value, context) {
  try {
    if (typeof ref == "function") {
      let n = typeof ref._unmount == "function";
      if (n) {
        ref._unmount();
      }
      if (!(n && value == null)) {
        ref._unmount = ref(value);
      }
    } else {
      ref.current = value;
    }
  } catch (error) {
    defaultOptionsRef._catchError(error, context);
  }
}
function unmountComponent(component, context, isRoot) {
  let refOrComponentOrChildren;
  if (defaultOptionsRef.unmount) {
    defaultOptionsRef.unmount(component);
  }
  if ((refOrComponentOrChildren = component.ref)) {
    if (
      !(
        refOrComponentOrChildren.current &&
        refOrComponentOrChildren.current !== component._dom
      )
    ) {
      applyReference(refOrComponentOrChildren, null, context);
    }
  }
  if ((refOrComponentOrChildren = component._component) != null) {
    if (refOrComponentOrChildren.componentWillUnmount) {
      try {
        refOrComponentOrChildren.componentWillUnmount();
      } catch (error) {
        defaultOptionsRef._catchError(error, context);
      }
    }
    refOrComponentOrChildren.base = refOrComponentOrChildren._parentDom = null;
  }
  if ((refOrComponentOrChildren = component._children)) {
    for (let index = 0; index < refOrComponentOrChildren.length; index++) {
      if (refOrComponentOrChildren[index]) {
        unmountComponent(
          refOrComponentOrChildren[index],
          context,
          isRoot || typeof component.type != "function"
        );
      }
    }
  }
  if (!isRoot) {
    removeDomNode(component._dom);
  }
  component._component = component._parent = component._dom = UNDEFINED_VALUE;
}
function renderElement(element, unusedParam, config) {
  return this.constructor(element, config);
}
function renderRootElement(rootElement, container, hydrateOrVirtualRoot) {
  if (container == document) {
    container = document.documentElement;
  }
  if (defaultOptionsRef._root) {
    defaultOptionsRef._root(rootElement, container);
  }
  let isHydrating = typeof hydrateOrVirtualRoot == "function";
  let previousVirtualRoot = isHydrating
    ? null
    : (hydrateOrVirtualRoot && hydrateOrVirtualRoot._children) ||
      container._children;
  let components = [];
  let referenceTriples = [];
  diffNodes(
    container,
    (rootElement = (
      (!isHydrating && hydrateOrVirtualRoot) ||
      container
    )._children =
      createVirtualElement(extractFragmentChildren, null, [rootElement])),
    previousVirtualRoot || EMPTY_OBJECT,
    EMPTY_OBJECT,
    container.namespaceURI,
    !isHydrating && hydrateOrVirtualRoot
      ? [hydrateOrVirtualRoot]
      : previousVirtualRoot
        ? null
        : container.firstChild
          ? sliceArray.call(container.childNodes)
          : null,
    components,
    !isHydrating && hydrateOrVirtualRoot
      ? hydrateOrVirtualRoot
      : previousVirtualRoot
        ? previousVirtualRoot._dom
        : container.firstChild,
    isHydrating,
    referenceTriples
  );
  commitRootChanges(components, rootElement, referenceTriples);
}
function hydrateElement(element, options) {
  renderRootElement(element, options, hydrateElement);
}
function cloneVirtualElement(originalElement, newProps, children) {
  let keyOverride, refOverride, propKey, defaultProps;
  let mergedProps = mergeObjects({}, originalElement.props);
  if (originalElement.type && originalElement.type.defaultProps) {
    defaultProps = originalElement.type.defaultProps;
  }
  for (propKey in newProps)
    if (propKey == "key") {
      keyOverride = newProps[propKey];
    } else if (propKey == "ref") {
      refOverride = newProps[propKey];
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
      arguments.length > 3 ? sliceArray.call(arguments, 2) : children;
  }
  return createVirtualNode(
    originalElement.type,
    mergedProps,
    keyOverride || originalElement.key,
    refOverride || originalElement.ref,
    null
  );
}
var temp = 0;
function createReactContext(defaultValue) {
  function provideContext(component) {
    if (!this.getChildContext) {
      let e = new Set();
      let childContext = {};
      childContext[provideContext._id] = this;
      this.getChildContext = () => childContext;
      this.componentWillUnmount = () => {
        e = null;
      };
      this.shouldComponentUpdate = function (nextProps) {
        if (this.props.value !== nextProps.value) {
          e.forEach((componentInstance) => {
            componentInstance._force = true;
            scheduleComponentRender(componentInstance);
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
    return component.children;
  }
  provideContext._id = "__cC" + temp++;
  provideContext._defaultValue = defaultValue;
  provideContext.Consumer = (parentNode, selector) =>
    parentNode.children(selector);
  provideContext.Provider =
    provideContext._contextRef =
    provideContext.Consumer.contextType =
      provideContext;
  return provideContext;
}
export {
  ComponentBase as Component,
  extractFragmentChildren as Fragment,
  cloneVirtualElement as cloneElement,
  createReactContext as createContext,
  createVirtualElement as createElement,
  createReference as createRef,
  createVirtualElement as h,
  hydrateElement as hydrate,
  isValidComponent as isValidElement,
  defaultOptionsRef as options,
  renderRootElement as render,
  flattenNonBooleanValues as toChildArray
};
