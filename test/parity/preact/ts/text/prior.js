var HYDRATE_MODE = 32;
var SUSPENDED_MODE = 128;
var INSERT_VNODE_FLAG = 65536;
var MATCHED_FLAG = 1 << 17;
var RESET_FLAGS = ~(HYDRATE_MODE | SUSPENDED_MODE);
var UNDEFINED_VALUE = undefined;
var EMPTY_OBJECT = {};
var EMPTY_ARRAY = [];
var NON_DIMENSIONAL_STYLE_REGEX = /acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i;
var checkIfArray = Array.isArray;
function mergeObjects(targetObject, sourceObject) {
  for (let nextState in sourceObject) targetObject[nextState] = sourceObject[nextState];
  return targetObject;
}
function removeDomNode(element) {
  if (element && element.parentNode) {
    element.parentNode.removeChild(element);
  }
}
var arraySlice = EMPTY_ARRAY.slice;
function handleError(error, currentComponent, unusedParam, errorInfo) {
  let componentInstance, ComponentConstructor, isDirty;
  for (; currentComponent = currentComponent._parent;) {
    if ((componentInstance = currentComponent._component) && !componentInstance._processingException) {
      try {
        ComponentConstructor = componentInstance.constructor;
        if (ComponentConstructor && ComponentConstructor.getDerivedStateFromError != null) {
          componentInstance.setState(ComponentConstructor.getDerivedStateFromError(error));
          isDirty = componentInstance._dirty;
        }
        if (componentInstance.componentDidCatch != null) {
          componentInstance.componentDidCatch(error, errorInfo || {});
          isDirty = componentInstance._dirty;
        }
        if (isDirty) {
          return componentInstance._pendingError = componentInstance;
        }
      } catch (caughtError) {
        error = caughtError;
      }
    }
  }
  throw error;
}
var defaultOptions = {
  _catchError: handleError
};
var defaultOptionsVal = defaultOptions;
var virtualNodeCounter = 0;
function createVirtualElement(componentType, props, firstChild) {
  let key, ref, propName;
  let propsObject = {};
  for (propName in props) if (propName == "key") {
    key = props[propName];
  } else if (propName == "ref") {
    ref = props[propName];
  } else {
    propsObject[propName] = props[propName];
  }
  if (arguments.length > 2) {
    propsObject.children = arguments.length > 3 ? arraySlice.call(arguments, 2) : firstChild;
  }
  if (typeof componentType == "function" && componentType.defaultProps != null) {
    for (propName in componentType.defaultProps) if (propsObject[propName] === UNDEFINED_VALUE) {
      propsObject[propName] = componentType.defaultProps[propName];
    }
  }
  return generateVirtualNode(componentType, propsObject, key, ref, null);
}
function generateVirtualNode(nodeType, nodeProps, nodeKey, nodeRef, originalId) {
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
    _original: originalId == null ? ++virtualNodeCounter : originalId,
    _index: -1,
    _flags: 0
  };
  if (originalId == null && defaultOptionsVal.vnode != null) {
    defaultOptionsVal.vnode(vnode);
  }
  return vnode;
}
function createReference() {
  return {
    current: null
  };
}
function getChildren(node) {
  return node.children;
}
var isElementValid = value => value != null && value.constructor == UNDEFINED_VALUE;
function ComponentBase(props, context) {
  this.props = props;
  this.context = context;
}
function findNextDomSibling(node, childIndex) {
  if (childIndex == null) {
    if (node._parent) {
      return findNextDomSibling(node._parent, node._index + 1);
    } else {
      return null;
    }
  }
  let childNode;
  for (; childIndex < node._children.length; childIndex++) {
    childNode = node._children[childIndex];
    if (childNode != null && childNode._dom != null) {
      return childNode._dom;
    }
  }
  if (typeof node.type == "function") {
    return findNextDomSibling(node);
  } else {
    return null;
  }
}
function handleComponentRendering(component) {
  let vnode = component._vnode;
  let domNode = vnode._dom;
  let pendingUpdates = [];
  let pendingRefs = [];
  if (component._parentDom) {
    const updatedVnode = mergeObjects({}, vnode);
    updatedVnode._original = vnode._original + 1;
    if (defaultOptionsVal.vnode) {
      defaultOptionsVal.vnode(updatedVnode);
    }
    reconcileNodes(component._parentDom, updatedVnode, vnode, component._globalContext, component._parentDom.namespaceURI, vnode._flags & HYDRATE_MODE ? [domNode] : null, pendingUpdates, domNode == null ? findNextDomSibling(vnode) : domNode, !!(vnode._flags & HYDRATE_MODE), pendingRefs);
    updatedVnode._original = vnode._original;
    updatedVnode._parent._children[updatedVnode._index] = updatedVnode;
    commitFiberRoot(pendingUpdates, updatedVnode, pendingRefs);
    if (updatedVnode._dom != domNode) {
      updateParentDomReference(updatedVnode);
    }
  }
}
function updateParentDomReference(parentNode) {
  if ((parentNode = parentNode._parent) != null && parentNode._component != null) {
    parentNode._dom = parentNode._component.base = null;
    for (let childIndex = 0; childIndex < parentNode._children.length; childIndex++) {
      let childNode = parentNode._children[childIndex];
      if (childNode != null && childNode._dom != null) {
        parentNode._dom = parentNode._component.base = childNode._dom;
        break;
      }
    }
    return updateParentDomReference(parentNode);
  }
}
ComponentBase.prototype.setState = function (newState, stateCallback) {
  let stateCopy;
  if (this._nextState != null && this._nextState !== this.state) {
    stateCopy = this._nextState;
  } else {
    stateCopy = this._nextState = mergeObjects({}, this.state);
  }
  if (typeof newState == "function") {
    newState = newState(mergeObjects({}, stateCopy), this.props);
  }
  if (newState) {
    mergeObjects(stateCopy, newState);
  }
  if (newState != null && this._vnode) {
    if (stateCallback) {
      this._stateCallbacks.push(stateCallback);
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
ComponentBase.prototype.render = getChildren;
var lastDebounceFn;
var componentRenderQueue = [];
var scheduleTask = typeof Promise == "function" ? Promise.prototype.then.bind(Promise.resolve()) : setTimeout;
function scheduleRender(component) {
  if (!component._dirty && (component._dirty = true) && componentRenderQueue.push(component) && !renderLoop._rerenderCount++ || lastDebounceFn !== defaultOptionsVal.debounceRendering) {
    ((lastDebounceFn = defaultOptionsVal.debounceRendering) || scheduleTask)(renderLoop);
  }
}
var sortByDepth = (nodeA, nodeB) => nodeA._vnode._depth - nodeB._vnode._depth;
function renderLoop() {
  let component;
  for (componentRenderQueue.sort(sortByDepth); component = componentRenderQueue.shift();) {
    if (component._dirty) {
      let previousLength = componentRenderQueue.length;
      handleComponentRendering(component);
      if (componentRenderQueue.length > previousLength) {
        componentRenderQueue.sort(sortByDepth);
      }
    }
  }
  renderLoop._rerenderCount = 0;
}
function compareAndUpdateChildren(parentVNode, newChildren, parent, oldChildren, namespace, componentInstance, context, parentDom, nextSibling, refQueue, refUpdates) {
  let index, prevChildData, currentChild, childDom, firstDomNode;
  let previousChildren = oldChildren && oldChildren._children || EMPTY_ARRAY;
  let childrenCount = newChildren.length;
  for (parent._nextDom = nextSibling, buildChildrenArray(parent, newChildren, previousChildren), nextSibling = parent._nextDom, index = 0; index < childrenCount; index++) {
    currentChild = parent._children[index];
    if (currentChild != null) {
      if (-1 === currentChild._index) {
        prevChildData = EMPTY_OBJECT;
      } else {
        prevChildData = previousChildren[currentChild._index] || EMPTY_OBJECT;
      }
      currentChild._index = index;
      reconcileNodes(parentVNode, currentChild, prevChildData, namespace, componentInstance, context, parentDom, nextSibling, refQueue, refUpdates);
      childDom = currentChild._dom;
      if (currentChild.ref && prevChildData.ref != currentChild.ref) {
        if (prevChildData.ref) {
          setRef(prevChildData.ref, null, currentChild);
        }
        refUpdates.push(currentChild.ref, currentChild._component || childDom, currentChild);
      }
      if (firstDomNode == null && childDom != null) {
        firstDomNode = childDom;
      }
      if (currentChild._flags & INSERT_VNODE_FLAG || prevChildData._children === currentChild._children) {
        nextSibling = insertNode(currentChild, nextSibling, parentVNode);
      } else if (typeof currentChild.type == "function" && currentChild._nextDom !== UNDEFINED_VALUE) {
        nextSibling = currentChild._nextDom;
      } else if (childDom) {
        nextSibling = childDom.nextSibling;
      }
      currentChild._nextDom = UNDEFINED_VALUE;
      currentChild._flags &= ~(INSERT_VNODE_FLAG | MATCHED_FLAG);
    }
  }
  parent._nextDom = nextSibling;
  parent._dom = firstDomNode;
}
function buildChildrenArray(parentNode, newChildren, oldChildren) {
  let index, child, matchedChild;
  const currentIndex = newChildren.length;
  let matchedIndex = oldChildren.length;
  let remainingCount = matchedIndex;
  let offset = 0;
  for (parentNode._children = [], index = 0; index < currentIndex; index++) {
    child = newChildren[index];
    if (child == null || typeof child == "boolean" || typeof child == "function") {
      child = parentNode._children[index] = null;
      continue;
    }
    if (typeof child == "string" || typeof child == "number" || typeof child == "bigint" || child.constructor == String) {
      child = parentNode._children[index] = generateVirtualNode(null, child, null, null, null);
    } else if (checkIfArray(child)) {
      child = parentNode._children[index] = generateVirtualNode(getChildren, {
        children: child
      }, null, null, null);
    } else if (child.constructor === UNDEFINED_VALUE && child._depth > 0) {
      child = parentNode._children[index] = generateVirtualNode(child.type, child.props, child.key, child.ref ? child.ref : null, child._original);
    } else {
      child = parentNode._children[index] = child;
    }
    const adjustedIndex = index + offset;
    child._parent = parentNode;
    child._depth = parentNode._depth + 1;
    const matchedOldIndex = child._index = locateMatchingElementIndex(child, oldChildren, adjustedIndex, remainingCount);
    matchedChild = null;
    if (-1 !== matchedOldIndex) {
      matchedChild = oldChildren[matchedOldIndex];
      remainingCount--;
      if (matchedChild) {
        matchedChild._flags |= MATCHED_FLAG;
      }
    }
    if (matchedChild == null || matchedChild._original === null) {
      if (-1 == matchedOldIndex) {
        offset--;
      }
      if (typeof child.type != "function") {
        child._flags |= INSERT_VNODE_FLAG;
      }
    } else if (matchedOldIndex !== adjustedIndex) {
      if (matchedOldIndex == adjustedIndex - 1) {
        offset--;
      } else if (matchedOldIndex == adjustedIndex + 1) {
        offset++;
      } else {
        if (matchedOldIndex > adjustedIndex) {
          offset--;
        } else {
          offset++;
        }
        child._flags |= INSERT_VNODE_FLAG;
      }
    }
  }
  if (remainingCount) {
    for (index = 0; index < matchedIndex; index++) {
      matchedChild = oldChildren[index];
      if (matchedChild != null && (matchedChild._flags & MATCHED_FLAG) === 0) {
        if (matchedChild._dom == parentNode._nextDom) {
          parentNode._nextDom = findNextDomSibling(matchedChild);
        }
        unmountComponent(matchedChild, matchedChild);
      }
    }
  }
}
function insertNode(node, refNode, parentContainer) {
  if (typeof node.type == "function") {
    let children = node._children;
    for (let index = 0; children && index < children.length; index++) {
      if (children[index]) {
        children[index]._parent = node;
        refNode = insertNode(children[index], refNode, parentContainer);
      }
    }
    return refNode;
  }
  if (node._dom != refNode) {
    if (refNode && node.type && !parentContainer.contains(refNode)) {
      refNode = findNextDomSibling(node);
    }
    parentContainer.insertBefore(node._dom, refNode || null);
    refNode = node._dom;
  }
  do {
    refNode = refNode && refNode.nextSibling;
  } while (refNode != null && refNode.nodeType === 8);
  return refNode;
}
function collectChildNodes(node, accumulator) {
  accumulator = accumulator || [];
  if (!(node == null || typeof node == "boolean")) {
    if (checkIfArray(node)) {
      node.some(childNode => {
        collectChildNodes(childNode, accumulator);
      });
    } else {
      accumulator.push(node);
    }
  }
  return accumulator;
}
function locateMatchingElementIndex(currentNode, siblingArray, currentIndex, depthThreshold) {
  const nodeKey = currentNode.key;
  const nodeType = currentNode.type;
  let prevIndex = currentIndex - 1;
  let nextIndex = currentIndex + 1;
  let siblingNode = siblingArray[currentIndex];
  let shouldSearchNeighbors = (typeof nodeType != "function" || nodeType === getChildren || nodeKey) && depthThreshold > (siblingNode != null && (siblingNode._flags & MATCHED_FLAG) === 0 ? 1 : 0);
  if (siblingNode === null || siblingNode && nodeKey == siblingNode.key && nodeType === siblingNode.type && (siblingNode._flags & MATCHED_FLAG) === 0) {
    return currentIndex;
  }
  if (shouldSearchNeighbors) {
    for (; prevIndex >= 0 || nextIndex < siblingArray.length;) {
      if (prevIndex >= 0) {
        siblingNode = siblingArray[prevIndex];
        if (siblingNode && (siblingNode._flags & MATCHED_FLAG) === 0 && nodeKey == siblingNode.key && nodeType === siblingNode.type) {
          return prevIndex;
        }
        prevIndex--;
      }
      if (nextIndex < siblingArray.length) {
        siblingNode = siblingArray[nextIndex];
        if (siblingNode && (siblingNode._flags & MATCHED_FLAG) === 0 && nodeKey == siblingNode.key && nodeType === siblingNode.type) {
          return nextIndex;
        }
        nextIndex++;
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
  } else if (typeof value != "number" || NON_DIMENSIONAL_STYLE_REGEX.test(propertyName)) {
    styleObj[propertyName] = value;
  } else {
    styleObj[propertyName] = value + "px";
  }
}
renderLoop._rerenderCount = 0;
var attachmentClock = 0;
function setElementProperty(targetElement, attributeName, value, oldValue, namespace) {
  let isCapture;
  e: if (attributeName === "style") {
    if (typeof value == "string") {
      targetElement.style.cssText = value;
    } else {
      if (typeof oldValue == "string") {
        targetElement.style.cssText = oldValue = "";
      }
      if (oldValue) {
        for (attributeName in oldValue) if (!(value && attributeName in value)) {
          applyStyle(targetElement.style, attributeName, "");
        }
      }
      if (value) {
        for (attributeName in value) if (!(oldValue && value[attributeName] === oldValue[attributeName])) {
          applyStyle(targetElement.style, attributeName, value[attributeName]);
        }
      }
    }
  } else if (attributeName[0] === "o" && attributeName[1] === "n") {
    isCapture = attributeName !== (attributeName = attributeName.replace(/(PointerCapture)$|Capture$/i, "$1"));
    if (attributeName.toLowerCase() in targetElement || attributeName === "onFocusOut" || attributeName === "onFocusIn") {
      attributeName = attributeName.toLowerCase().slice(2);
    } else {
      attributeName = attributeName.slice(2);
    }
    if (!targetElement._listeners) {
      targetElement._listeners = {};
    }
    targetElement._listeners[attributeName + isCapture] = value;
    if (value) {
      if (oldValue) {
        value._attached = oldValue._attached;
      } else {
        value._attached = attachmentClock;
        targetElement.addEventListener(attributeName, isCapture ? eventCaptureHandler : eventHandler, isCapture);
      }
    } else {
      targetElement.removeEventListener(attributeName, isCapture ? eventCaptureHandler : eventHandler, isCapture);
    }
  } else {
    if (namespace == "http://www.w3.org/2000/svg") {
      attributeName = attributeName.replace(/xlink(H|:h)/, "h").replace(/sName$/, "s");
    } else if (attributeName != "width" && attributeName != "height" && attributeName != "href" && attributeName != "list" && attributeName != "form" && attributeName != "tabIndex" && attributeName != "download" && attributeName != "rowSpan" && attributeName != "colSpan" && attributeName != "role" && attributeName != "popover" && attributeName in targetElement) {
      try {
        targetElement[attributeName] = value == null ? "" : value;
        break e;
      } catch (styleProcessing) {}
    }
    if (!(typeof value == "function")) {
      if (value == null || value === false && attributeName[4] !== "-") {
        targetElement.removeAttribute(attributeName);
      } else {
        targetElement.setAttribute(attributeName, attributeName == "popover" && value == 1 ? "" : value);
      }
    }
  }
}
function createEventListenerProxy(eventKeySuffix) {
  return function (event) {
    if (this._listeners) {
      const listener = this._listeners[event.type + eventKeySuffix];
      if (event._dispatched == null) {
        event._dispatched = attachmentClock++;
      } else if (event._dispatched < listener._attached) {
        return;
      }
      if (defaultOptionsVal.event) {
        event = defaultOptionsVal.event(event);
      }
      if ("handleEvent" in listener) {
        return listener.handleEvent(event);
      } else {
        return listener(event);
      }
    }
  };
}
var eventHandler = createEventListenerProxy(false);
var eventCaptureHandler = createEventListenerProxy(true);
function reconcileNodes(parentDom, vnode, componentInstance, globalContext, namespaceURI, domArray, callbackQueue, context, hydrateMode, renderCallbacks) {
  let postDiffHook;
  let componentIndex = vnode.type;
  if (vnode.constructor !== UNDEFINED_VALUE) {
    return null;
  }
  if (componentInstance._flags & SUSPENDED_MODE) {
    hydrateMode = !!(componentInstance._flags & HYDRATE_MODE);
    domArray = [context = vnode._dom = componentInstance._dom];
  }
  if (postDiffHook = defaultOptionsVal._diff) {
    postDiffHook(vnode);
  }
  e: if (typeof componentIndex == "function") {
    try {
      let currentComponent, placeholderObject, renderedNode, componentState, snapshot, processingException;
      let props = vnode.props;
      const hasRenderMethod = "prototype" in componentIndex && componentIndex.prototype.render;
      postDiffHook = componentIndex.contextType;
      let contextProvider = postDiffHook && globalContext[postDiffHook._id];
      let childContext = postDiffHook ? contextProvider ? contextProvider.props.value : postDiffHook._defaultValue : globalContext;
      if (componentInstance._component) {
        currentComponent = vnode._component = componentInstance._component;
        processingException = currentComponent._processingException = currentComponent._pendingError;
      } else {
        if (hasRenderMethod) {
          vnode._component = currentComponent = new componentIndex(props, childContext);
        } else {
          vnode._component = currentComponent = new ComponentBase(props, childContext);
          currentComponent.constructor = componentIndex;
          currentComponent.render = renderElement;
        }
        if (contextProvider) {
          contextProvider.sub(currentComponent);
        }
        currentComponent.props = props;
        if (!currentComponent.state) {
          currentComponent.state = {};
        }
        currentComponent.context = childContext;
        currentComponent._globalContext = globalContext;
        placeholderObject = currentComponent._dirty = true;
        currentComponent._renderCallbacks = [];
        currentComponent._stateCallbacks = [];
      }
      if (hasRenderMethod && currentComponent._nextState == null) {
        currentComponent._nextState = currentComponent.state;
      }
      if (hasRenderMethod && componentIndex.getDerivedStateFromProps != null) {
        if (currentComponent._nextState == currentComponent.state) {
          currentComponent._nextState = mergeObjects({}, currentComponent._nextState);
        }
        mergeObjects(currentComponent._nextState, componentIndex.getDerivedStateFromProps(props, currentComponent._nextState));
      }
      renderedNode = currentComponent.props;
      componentState = currentComponent.state;
      currentComponent._vnode = vnode;
      if (placeholderObject) {
        if (hasRenderMethod && componentIndex.getDerivedStateFromProps == null && currentComponent.componentWillMount != null) {
          currentComponent.componentWillMount();
        }
        if (hasRenderMethod && currentComponent.componentDidMount != null) {
          currentComponent._renderCallbacks.push(currentComponent.componentDidMount);
        }
      } else {
        if (hasRenderMethod && componentIndex.getDerivedStateFromProps == null && props !== renderedNode && currentComponent.componentWillReceiveProps != null) {
          currentComponent.componentWillReceiveProps(props, childContext);
        }
        if (!currentComponent._force && (currentComponent.shouldComponentUpdate != null && false === currentComponent.shouldComponentUpdate(props, currentComponent._nextState, childContext) || vnode._original === componentInstance._original)) {
          if (vnode._original !== componentInstance._original) {
            currentComponent.props = props;
            currentComponent.state = currentComponent._nextState;
            currentComponent._dirty = false;
          }
          vnode._dom = componentInstance._dom;
          vnode._children = componentInstance._children;
          vnode._children.some(childNode => {
            if (childNode) {
              childNode._parent = vnode;
            }
          });
          for (let stateCallbackIndex = 0; stateCallbackIndex < currentComponent._stateCallbacks.length; stateCallbackIndex++) {
            currentComponent._renderCallbacks.push(currentComponent._stateCallbacks[stateCallbackIndex]);
          }
          currentComponent._stateCallbacks = [];
          if (currentComponent._renderCallbacks.length) {
            callbackQueue.push(currentComponent);
          }
          break e;
        }
        if (currentComponent.componentWillUpdate != null) {
          currentComponent.componentWillUpdate(props, currentComponent._nextState, childContext);
        }
        if (hasRenderMethod && currentComponent.componentDidUpdate != null) {
          currentComponent._renderCallbacks.push(() => {
            currentComponent.componentDidUpdate(renderedNode, componentState, snapshot);
          });
        }
      }
      currentComponent.context = childContext;
      currentComponent.props = props;
      currentComponent._parentDom = parentDom;
      currentComponent._force = false;
      let renderHook = defaultOptionsVal._render;
      let renderIteration = 0;
      if (hasRenderMethod) {
        currentComponent.state = currentComponent._nextState;
        currentComponent._dirty = false;
        if (renderHook) {
          renderHook(vnode);
        }
        postDiffHook = currentComponent.render(currentComponent.props, currentComponent.state, currentComponent.context);
        for (let stateCallbackIndex2 = 0; stateCallbackIndex2 < currentComponent._stateCallbacks.length; stateCallbackIndex2++) {
          currentComponent._renderCallbacks.push(currentComponent._stateCallbacks[stateCallbackIndex2]);
        }
        currentComponent._stateCallbacks = [];
      } else {
        do {
          currentComponent._dirty = false;
          if (renderHook) {
            renderHook(vnode);
          }
          postDiffHook = currentComponent.render(currentComponent.props, currentComponent.state, currentComponent.context);
          currentComponent.state = currentComponent._nextState;
        } while (currentComponent._dirty && ++renderIteration < 25);
      }
      currentComponent.state = currentComponent._nextState;
      if (currentComponent.getChildContext != null) {
        globalContext = mergeObjects(mergeObjects({}, globalContext), currentComponent.getChildContext());
      }
      if (hasRenderMethod && !placeholderObject && currentComponent.getSnapshotBeforeUpdate != null) {
        snapshot = currentComponent.getSnapshotBeforeUpdate(renderedNode, componentState);
      }
      let renderedOutput = postDiffHook != null && postDiffHook.type === getChildren && postDiffHook.key == null ? postDiffHook.props.children : postDiffHook;
      compareAndUpdateChildren(parentDom, checkIfArray(renderedOutput) ? renderedOutput : [renderedOutput], vnode, componentInstance, globalContext, namespaceURI, domArray, callbackQueue, context, hydrateMode, renderCallbacks);
      currentComponent.base = vnode._dom;
      vnode._flags &= RESET_FLAGS;
      if (currentComponent._renderCallbacks.length) {
        callbackQueue.push(currentComponent);
      }
      if (processingException) {
        currentComponent._pendingError = currentComponent._processingException = null;
      }
    } catch (catchError) {
      vnode._original = null;
      if (hydrateMode || domArray != null) {
        for (vnode._flags |= hydrateMode ? HYDRATE_MODE | SUSPENDED_MODE : SUSPENDED_MODE; context && context.nodeType === 8 && context.nextSibling;) {
          context = context.nextSibling;
        }
        domArray[domArray.indexOf(context)] = null;
        vnode._dom = context;
      } else {
        vnode._dom = componentInstance._dom;
        vnode._children = componentInstance._children;
      }
      defaultOptionsVal._catchError(catchError, vnode, componentInstance);
    }
  } else if (domArray == null && vnode._original === componentInstance._original) {
    vnode._children = componentInstance._children;
    vnode._dom = componentInstance._dom;
  } else {
    vnode._dom = updateElementNode(componentInstance._dom, vnode, componentInstance, globalContext, namespaceURI, domArray, callbackQueue, hydrateMode, renderCallbacks);
  }
  if (postDiffHook = defaultOptionsVal.diffed) {
    postDiffHook(vnode);
  }
}
function commitFiberRoot(root, fiber, refs) {
  fiber._nextDom = UNDEFINED_VALUE;
  for (let loopIndex = 0; loopIndex < refs.length; loopIndex++) {
    setRef(refs[loopIndex], refs[++loopIndex], refs[++loopIndex]);
  }
  if (defaultOptionsVal._commit) {
    defaultOptionsVal._commit(fiber, root);
  }
  root.some(component => {
    try {
      root = component._renderCallbacks;
      component._renderCallbacks = [];
      root.some(callback => {
        callback.call(component);
      });
    } catch (callback) {
      defaultOptionsVal._catchError(callback, component._vnode);
    }
  });
}
function updateElementNode(domElement, vnode, prevVnode, parent, namespace, childNodes, key, isHydrating, options) {
  let index, innerHtmlContent, innerHtmlPlaceholder, childrenArray, attribute, newValue, newChecked;
  let oldProps = prevVnode.props;
  let newProps = vnode.props;
  let elementType = vnode.type;
  if (elementType === "svg") {
    namespace = "http://www.w3.org/2000/svg";
  } else if (elementType === "math") {
    namespace = "http://www.w3.org/1998/Math/MathML";
  } else if (!namespace) {
    namespace = "http://www.w3.org/1999/xhtml";
  }
  if (childNodes != null) {
    for (index = 0; index < childNodes.length; index++) {
      attribute = childNodes[index];
      if (attribute && "setAttribute" in attribute == !!elementType && (elementType ? attribute.localName === elementType : attribute.nodeType === 3)) {
        domElement = attribute;
        childNodes[index] = null;
        break;
      }
    }
  }
  if (domElement == null) {
    if (elementType === null) {
      return document.createTextNode(newProps);
    }
    domElement = document.createElementNS(namespace, elementType, newProps.is && newProps);
    if (isHydrating) {
      if (defaultOptionsVal._hydrationMismatch) {
        defaultOptionsVal._hydrationMismatch(vnode, childNodes);
      }
      isHydrating = false;
    }
    childNodes = null;
  }
  if (elementType === null) {
    if (!(oldProps === newProps || isHydrating && domElement.data === newProps)) {
      domElement.data = newProps;
    }
  } else {
    childNodes = childNodes && arraySlice.call(domElement.childNodes);
    oldProps = prevVnode.props || EMPTY_OBJECT;
    if (!isHydrating && childNodes != null) {
      for (oldProps = {}, index = 0; index < domElement.attributes.length; index++) {
        attribute = domElement.attributes[index];
        oldProps[attribute.name] = attribute.value;
      }
    }
    for (index in oldProps) {
      attribute = oldProps[index];
      if (index == "children") {
        ;
      } else if (index == "dangerouslySetInnerHTML") {
        innerHtmlPlaceholder = attribute;
      } else if (!(index in newProps)) {
        if (index == "value" && "defaultValue" in newProps || index == "checked" && "defaultChecked" in newProps) {
          continue;
        }
        setElementProperty(domElement, index, null, attribute, namespace);
      }
    }
    for (index in newProps) {
      attribute = newProps[index];
      if (index == "children") {
        childrenArray = attribute;
      } else if (index == "dangerouslySetInnerHTML") {
        innerHtmlContent = attribute;
      } else if (index == "value") {
        newValue = attribute;
      } else if (index == "checked") {
        newChecked = attribute;
      } else if (!(isHydrating && typeof attribute != "function" || oldProps[index] === attribute)) {
        setElementProperty(domElement, index, attribute, oldProps[index], namespace);
      }
    }
    if (innerHtmlContent) {
      if (!(isHydrating || innerHtmlPlaceholder && (innerHtmlContent.__html === innerHtmlPlaceholder.__html || innerHtmlContent.__html === domElement.innerHTML))) {
        domElement.innerHTML = innerHtmlContent.__html;
      }
      vnode._children = [];
    } else {
      if (innerHtmlPlaceholder) {
        domElement.innerHTML = "";
      }
      compareAndUpdateChildren(domElement, checkIfArray(childrenArray) ? childrenArray : [childrenArray], vnode, prevVnode, parent, elementType === "foreignObject" ? "http://www.w3.org/1999/xhtml" : namespace, childNodes, key, childNodes ? childNodes[0] : prevVnode._children && findNextDomSibling(prevVnode, 0), isHydrating, options);
      if (childNodes != null) {
        for (index = childNodes.length; index--;) {
          removeDomNode(childNodes[index]);
        }
      }
    }
    if (!isHydrating) {
      index = "value";
      if (elementType === "progress" && newValue == null) {
        domElement.removeAttribute("value");
      } else if (newValue !== UNDEFINED_VALUE && (newValue !== domElement[index] || elementType === "progress" && !newValue || elementType === "option" && newValue !== oldProps[index])) {
        setElementProperty(domElement, index, newValue, oldProps[index], namespace);
      }
      index = "checked";
      if (newChecked !== UNDEFINED_VALUE && newChecked !== domElement[index]) {
        setElementProperty(domElement, index, newChecked, oldProps[index], namespace);
      }
    }
  }
  return domElement;
}
function setRef(refOrCallback, value, context) {
  try {
    if (typeof refOrCallback == "function") {
      let isUnmountable = typeof refOrCallback._unmount == "function";
      if (isUnmountable) {
        refOrCallback._unmount();
      }
      if (!(isUnmountable && value == null)) {
        refOrCallback._unmount = refOrCallback(value);
      }
    } else {
      refOrCallback.current = value;
    }
  } catch (error) {
    defaultOptionsVal._catchError(error, context);
  }
}
function unmountComponent(componentInstance, parentContext, isRoot) {
  let refObj;
  if (defaultOptionsVal.unmount) {
    defaultOptionsVal.unmount(componentInstance);
  }
  if (refObj = componentInstance.ref) {
    if (!(refObj.current && refObj.current !== componentInstance._dom)) {
      setRef(refObj, null, parentContext);
    }
  }
  if ((refObj = componentInstance._component) != null) {
    if (refObj.componentWillUnmount) {
      try {
        refObj.componentWillUnmount();
      } catch (componentInstanceVal) {
        defaultOptionsVal._catchError(componentInstanceVal, parentContext);
      }
    }
    refObj.base = refObj._parentDom = null;
  }
  if (refObj = componentInstance._children) {
    for (let childIndex = 0; childIndex < refObj.length; childIndex++) {
      if (refObj[childIndex]) {
        unmountComponent(refObj[childIndex], parentContext, isRoot || typeof componentInstance.type != "function");
      }
    }
  }
  if (!isRoot) {
    removeDomNode(componentInstance._dom);
  }
  componentInstance._component = componentInstance._parent = componentInstance._dom = componentInstance._nextDom = UNDEFINED_VALUE;
}
function renderElement(element, renderOptions, context) {
  return this.constructor(element, context);
}
function renderElementVal(component, container, hydrateOption) {
  if (defaultOptionsVal._root) {
    defaultOptionsVal._root(component, container);
  }
  let isHydrate = typeof hydrateOption == "function";
  let children = isHydrate ? null : hydrateOption && hydrateOption._children || container._children;
  let refs = [];
  let pendingRefs = [];
  reconcileNodes(container, component = (!isHydrate && hydrateOption || container)._children = createVirtualElement(getChildren, null, [component]), children || EMPTY_OBJECT, EMPTY_OBJECT, container.namespaceURI, !isHydrate && hydrateOption ? [hydrateOption] : children ? null : container.firstChild ? arraySlice.call(container.childNodes) : null, refs, !isHydrate && hydrateOption ? hydrateOption : children ? children._dom : container.firstChild, isHydrate, pendingRefs);
  commitFiberRoot(refs, component, pendingRefs);
}
function hydrateComponent(component, context) {
  renderElementVal(component, context, hydrateComponent);
}
function cloneVirtualElement(sourceElement, props, children) {
  let key, ref, propName, defaultProps;
  let mergedProps = mergeObjects({}, sourceElement.props);
  if (sourceElement.type && sourceElement.type.defaultProps) {
    defaultProps = sourceElement.type.defaultProps;
  }
  for (propName in props) if (propName == "key") {
    key = props[propName];
  } else if (propName == "ref") {
    ref = props[propName];
  } else if (props[propName] === UNDEFINED_VALUE && defaultProps !== UNDEFINED_VALUE) {
    mergedProps[propName] = defaultProps[propName];
  } else {
    mergedProps[propName] = props[propName];
  }
  if (arguments.length > 2) {
    mergedProps.children = arguments.length > 3 ? arraySlice.call(arguments, 2) : children;
  }
  return generateVirtualNode(sourceElement.type, mergedProps, key || sourceElement.key, ref || sourceElement.ref, null);
}
var element = 0;
function createContextObject(defaultValue, contextName) {
  const context = {
    _id: contextName = "__cC" + element++,
    _defaultValue: defaultValue,
    Consumer: (parent, childSelector) => parent.children(childSelector),
    Provider(children) {
      if (!this.getChildContext) {
        let subscriberSet = new Set();
        let contextData = {};
        contextData[contextName] = this;
        this.getChildContext = () => contextData;
        this.componentWillUnmount = () => {
          subscriberSet = null;
        };
        this.shouldComponentUpdate = function (nextProps) {
          if (this.props.value !== nextProps.value) {
            subscriberSet.forEach(component => {
              component._force = true;
              scheduleRender(component);
            });
          }
        };
        this.sub = subscriber => {
          subscriberSet.add(subscriber);
          let originalUnmount = subscriber.componentWillUnmount;
          subscriber.componentWillUnmount = () => {
            if (subscriberSet) {
              subscriberSet.delete(subscriber);
            }
            if (originalUnmount) {
              originalUnmount.call(subscriber);
            }
          };
        };
      }
      return children.children;
    }
  };
  return context.Provider._contextRef = context.Consumer.contextType = context;
}
export { ComponentBase as Component, getChildren as Fragment, cloneVirtualElement as cloneElement, createContextObject as createContext, createVirtualElement as createElement, createReference as createRef, createVirtualElement as h, hydrateComponent as hydrate, isElementValid as isValidElement, defaultOptionsVal as options, renderElementVal as render, collectChildNodes as toChildArray };