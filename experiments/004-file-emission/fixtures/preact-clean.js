const MODE_HYDRATE = 32;
const MODE_SUSPENDED = 128;
const INSERT_VNODE = 65536;
const MATCHED = 131072;
const RESET_MODE = -161;
const EMPTY_OBJ = {};
const EMPTY_ARR = [];
const IS_NON_DIMENSIONAL =
  /acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i;
const isArray = Array.isArray;
function assign(targetObj, sourceObj) {
  for (let newState in sourceObj) {
    targetObj[newState] = sourceObj[newState];
  }
  return targetObj;
}
function removeNode(node) {
  if (node && node.parentNode) {
    node.parentNode.removeChild(node);
  }
}
const slice = EMPTY_ARR.slice;
function _catchError(error, component, unused, errorInfo) {
  let currentInstance;
  let componentConstructor;
  let isDirty;
  while ((component = component._parent)) {
    if (
      (currentInstance = component._component) &&
      !currentInstance._processingException
    ) {
      try {
        componentConstructor = currentInstance.constructor;
        if (
          componentConstructor &&
          componentConstructor.getDerivedStateFromError != null
        ) {
          currentInstance.setState(
            componentConstructor.getDerivedStateFromError(error),
          );
          isDirty = currentInstance._dirty;
        }
        if (currentInstance.componentDidCatch != null) {
          currentInstance.componentDidCatch(error, errorInfo || {});
          isDirty = currentInstance._dirty;
        }
        if (isDirty) {
          return (currentInstance._pendingError = currentInstance);
        }
      } catch (n) {
        error = n;
      }
    }
  }
  throw error;
}
const options$1 = {
  _catchError: _catchError,
};
let vnodeId = 0;
function createElement(vnodeType, props, children) {
  let key;
  let ref;
  let propName;
  let finalProps = {};
  for (propName in props) {
    if (propName == "key") {
      key = props[propName];
    } else if (propName == "ref") {
      ref = props[propName];
    } else {
      finalProps[propName] = props[propName];
    }
  }
  if (arguments.length > 2) {
    finalProps.children =
      arguments.length > 3 ? slice.call(arguments, 2) : children;
  }
  if (typeof vnodeType == "function" && vnodeType.defaultProps != null) {
    for (propName in vnodeType.defaultProps) {
      if (finalProps[propName] === undefined) {
        finalProps[propName] = vnodeType.defaultProps[propName];
      }
    }
  }
  return createVNode(vnodeType, finalProps, key, ref, null);
}
function createVNode(vnodeType, vnodeProps, vnodeKey, vnodeRef, originalId) {
  const vnodeInstance = {
    type: vnodeType,
    props: vnodeProps,
    key: vnodeKey,
    ref: vnodeRef,
    _children: null,
    _parent: null,
    _depth: 0,
    _dom: null,
    _nextDom: undefined,
    _component: null,
    constructor: undefined,
    _original: originalId == null ? ++vnodeId : originalId,
    _index: -1,
    _flags: 0,
  };
  if (originalId == null && options$1.vnode != null) {
    options$1.vnode(vnodeInstance);
  }
  return vnodeInstance;
}
function createRef() {
  return {
    current: null,
  };
}
function Fragment(props) {
  return props.children;
}
const isValidElement = (isObjectWithoutPrototype) =>
  isObjectWithoutPrototype != null &&
  isObjectWithoutPrototype.constructor == null;
function BaseComponent(props, context) {
  this.props = props;
  this.context = context;
}
function getDomSibling(node, childIndex) {
  if (childIndex == null) {
    if (node._parent) {
      return getDomSibling(node._parent, node._index + 1);
    } else {
      return null;
    }
  }
  let child;
  for (; childIndex < node._children.length; childIndex++) {
    child = node._children[childIndex];
    if (child != null && child._dom != null) {
      return child._dom;
    }
  }
  if (typeof node.type == "function") {
    return getDomSibling(node);
  } else {
    return null;
  }
}
function renderComponent(component) {
  let virtualNode = component._vnode;
  let domElement = virtualNode._dom;
  let diffComponents = [];
  let refData = [];
  if (component._parentDom) {
    const virtualNodeClone = assign({}, virtualNode);
    virtualNodeClone._original = virtualNode._original + 1;
    if (options$1.vnode) {
      options$1.vnode(virtualNodeClone);
    }
    diff(
      component._parentDom,
      virtualNodeClone,
      virtualNode,
      component._globalContext,
      component._parentDom.namespaceURI,
      virtualNode._flags & 32 ? [domElement] : null,
      diffComponents,
      domElement == null ? getDomSibling(virtualNode) : domElement,
      !!(virtualNode._flags & 32),
      refData,
    );
    virtualNodeClone._original = virtualNode._original;
    virtualNodeClone._parent._children[virtualNodeClone._index] =
      virtualNodeClone;
    commitRoot(diffComponents, virtualNodeClone, refData);
    if (virtualNodeClone._dom != domElement) {
      updateParentDomPointers(virtualNodeClone);
    }
  }
}
function updateParentDomPointers(parentNode) {
  if (
    (parentNode = parentNode._parent) != null &&
    parentNode._component != null
  ) {
    parentNode._dom = parentNode._component.base = null;
    for (
      let childIndex = 0;
      childIndex < parentNode._children.length;
      childIndex++
    ) {
      let childVNode = parentNode._children[childIndex];
      if (childVNode != null && childVNode._dom != null) {
        parentNode._dom = parentNode._component.base = childVNode._dom;
        break;
      }
    }
    return updateParentDomPointers(parentNode);
  }
}
BaseComponent.prototype.setState = function (nextState, stateCallback) {
  let newState;
  if (this._nextState != null && this._nextState !== this.state) {
    newState = this._nextState;
  } else {
    newState = this._nextState = assign({}, this.state);
  }
  if (typeof nextState == "function") {
    nextState = nextState(assign({}, newState), this.props);
  }
  if (nextState) {
    assign(newState, nextState);
  }
  if (nextState != null && this._vnode) {
    if (stateCallback) {
      this._stateCallbacks.push(stateCallback);
    }
    enqueueRender(this);
  }
};
BaseComponent.prototype.forceUpdate = function (renderCallback) {
  if (this._vnode) {
    this._force = true;
    if (renderCallback) {
      this._renderCallbacks.push(renderCallback);
    }
    enqueueRender(this);
  }
};
BaseComponent.prototype.render = Fragment;
let prevDebounce;
let rerenderQueue = [];
const defer =
  typeof Promise == "function"
    ? Promise.prototype.then.bind(Promise.resolve())
    : setTimeout;
function enqueueRender(component) {
  if (
    (!component._dirty &&
      (component._dirty = true) &&
      rerenderQueue.push(component) &&
      !process._rerenderCount++) ||
    prevDebounce !== options$1.debounceRendering
  ) {
    prevDebounce = options$1.debounceRendering;
    (prevDebounce || defer)(process);
  }
}
const depthSort = (nodeA, nodeB) => nodeA._vnode._depth - nodeB._vnode._depth;
function process() {
  let component;
  for (rerenderQueue.sort(depthSort); (component = rerenderQueue.shift()); ) {
    if (component._dirty) {
      let previousQueueLength = rerenderQueue.length;
      renderComponent(component);
      if (rerenderQueue.length > previousQueueLength) {
        rerenderQueue.sort(depthSort);
      }
    }
  }
  process._rerenderCount = 0;
}
function diffChildren(e, n, t, o, r, l, i, _, u, s, c) {
  let a;
  let p;
  let f;
  let d;
  let h;
  let m = (o && o._children) || EMPTY_ARR;
  let g = n.length;
  t._nextDom = u;
  constructNewChildrenArray(t, n, m);
  u = t._nextDom;
  a = 0;
  for (; a < g; a++) {
    f = t._children[a];
    if (f != null) {
      if (f._index === -1) {
        p = EMPTY_OBJ;
      } else {
        p = m[f._index] || EMPTY_OBJ;
      }
      f._index = a;
      diff(e, f, p, r, l, i, _, u, s, c);
      d = f._dom;
      if (f.ref && p.ref != f.ref) {
        if (p.ref) {
          applyRef(p.ref, null, f);
        }
        c.push(f.ref, f._component || d, f);
      }
      if (h == null && d != null) {
        h = d;
      }
      if (f._flags & 65536 || p._children === f._children) {
        u = insert(f, u, e);
      } else if (typeof f.type == "function" && f._nextDom !== undefined) {
        u = f._nextDom;
      } else if (d) {
        u = d.nextSibling;
      }
      f._nextDom = undefined;
      f._flags &= -196609;
    }
  }
  t._nextDom = u;
  t._dom = h;
}
function constructNewChildrenArray(
  parentVNode,
  newChildrenArray,
  oldChildrenArray,
) {
  let childIndex;
  let childVNode;
  let oldChildVNode;
  const childrenIndexOrLength = newChildrenArray.length;
  let oldChildrenCountOrMatchIndex = oldChildrenArray.length;
  let remainingOldChildrenCount = oldChildrenCountOrMatchIndex;
  let indexOffset = 0;
  parentVNode._children = [];
  childIndex = 0;
  for (; childIndex < childrenIndexOrLength; childIndex++) {
    childVNode = newChildrenArray[childIndex];
    if (
      childVNode == null ||
      typeof childVNode == "boolean" ||
      typeof childVNode == "function"
    ) {
      childVNode = parentVNode._children[childIndex] = null;
      continue;
    }
    if (
      typeof childVNode == "string" ||
      typeof childVNode == "number" ||
      typeof childVNode == "bigint" ||
      childVNode.constructor == String
    ) {
      childVNode = parentVNode._children[childIndex] = createVNode(
        null,
        childVNode,
        null,
        null,
        null,
      );
    } else if (isArray(childVNode)) {
      childVNode = parentVNode._children[childIndex] = createVNode(
        Fragment,
        {
          children: childVNode,
        },
        null,
        null,
        null,
      );
    } else if (childVNode.constructor === undefined && childVNode._depth > 0) {
      childVNode = parentVNode._children[childIndex] = createVNode(
        childVNode.type,
        childVNode.props,
        childVNode.key,
        childVNode.ref ? childVNode.ref : null,
        childVNode._original,
      );
    } else {
      childVNode = parentVNode._children[childIndex] = childVNode;
    }
    const i = childIndex + indexOffset;
    childVNode._parent = parentVNode;
    childVNode._depth = parentVNode._depth + 1;
    const _ = (childVNode._index = findMatchingIndex(
      childVNode,
      oldChildrenArray,
      i,
      remainingOldChildrenCount,
    ));
    oldChildVNode = null;
    if (_ !== -1) {
      oldChildVNode = oldChildrenArray[_];
      remainingOldChildrenCount--;
      if (oldChildVNode) {
        oldChildVNode._flags |= 131072;
      }
    }
    if (oldChildVNode == null || oldChildVNode._original === null) {
      if (_ == -1) {
        indexOffset--;
      }
      if (typeof childVNode.type != "function") {
        childVNode._flags |= 65536;
      }
    } else if (_ !== i) {
      if (_ == i - 1) {
        indexOffset--;
      } else if (_ == i + 1) {
        indexOffset++;
      } else {
        if (_ > i) {
          indexOffset--;
        } else {
          indexOffset++;
        }
        childVNode._flags |= 65536;
      }
    }
  }
  if (remainingOldChildrenCount) {
    for (
      childIndex = 0;
      childIndex < oldChildrenCountOrMatchIndex;
      childIndex++
    ) {
      oldChildVNode = oldChildrenArray[childIndex];
      if (oldChildVNode != null && !(oldChildVNode._flags & 131072)) {
        if (oldChildVNode._dom == parentVNode._nextDom) {
          parentVNode._nextDom = getDomSibling(oldChildVNode);
        }
        unmount(oldChildVNode, oldChildVNode);
      }
    }
  }
}
function insert(virtualNode, nextSibling, parentDom) {
  if (typeof virtualNode.type == "function") {
    let children = virtualNode._children;
    for (let index = 0; children && index < children.length; index++) {
      if (children[index]) {
        children[index]._parent = virtualNode;
        nextSibling = insert(children[index], nextSibling, parentDom);
      }
    }
    return nextSibling;
  }
  if (virtualNode._dom != nextSibling) {
    if (nextSibling && virtualNode.type && !parentDom.contains(nextSibling)) {
      nextSibling = getDomSibling(virtualNode);
    }
    parentDom.insertBefore(virtualNode._dom, nextSibling || null);
    nextSibling = virtualNode._dom;
  }
  do {
    nextSibling = nextSibling && nextSibling.nextSibling;
  } while (nextSibling != null && nextSibling.nodeType === 8);
  return nextSibling;
}
function toChildArray(node, children) {
  children = children || [];
  if (node != null && typeof node != "boolean") {
    if (isArray(node)) {
      node.some((e) => {
        toChildArray(e, children);
      });
    } else {
      children.push(node);
    }
  }
  return children;
}
function findMatchingIndex(targetVNode, vnodeArray, startIndex, searchLimit) {
  const targetKey = targetVNode.key;
  const targetType = targetVNode.type;
  let leftIndex = startIndex - 1;
  let rightIndex = startIndex + 1;
  let currentVNode = vnodeArray[startIndex];
  let shouldSearchOutward =
    searchLimit >
    (currentVNode == null || currentVNode._flags & 131072 ? 0 : 1);
  if (
    currentVNode === null ||
    (currentVNode &&
      targetKey == currentVNode.key &&
      targetType === currentVNode.type &&
      !(currentVNode._flags & 131072))
  ) {
    return startIndex;
  }
  if (shouldSearchOutward) {
    while (leftIndex >= 0 || rightIndex < vnodeArray.length) {
      if (leftIndex >= 0) {
        currentVNode = vnodeArray[leftIndex];
        if (
          currentVNode &&
          !(currentVNode._flags & 131072) &&
          targetKey == currentVNode.key &&
          targetType === currentVNode.type
        ) {
          return leftIndex;
        }
        leftIndex--;
      }
      if (rightIndex < vnodeArray.length) {
        currentVNode = vnodeArray[rightIndex];
        if (
          currentVNode &&
          !(currentVNode._flags & 131072) &&
          targetKey == currentVNode.key &&
          targetType === currentVNode.type
        ) {
          return rightIndex;
        }
        rightIndex++;
      }
    }
  }
  return -1;
}
function setStyle(styleObj, propName, value) {
  if (propName[0] === "-") {
    styleObj.setProperty(propName, value == null ? "" : value);
  } else if (value == null) {
    styleObj[propName] = "";
  } else if (typeof value != "number" || IS_NON_DIMENSIONAL.test(propName)) {
    styleObj[propName] = value;
  } else {
    styleObj[propName] = value + "px";
  }
}
process._rerenderCount = 0;
let eventClock = 0;
function setProperty(element, propName, newValue, oldValue, namespace) {
  let isCapture;
  e: if (propName === "style") {
    if (typeof newValue == "string") {
      element.style.cssText = newValue;
    } else {
      if (typeof oldValue == "string") {
        element.style.cssText = oldValue = "";
      }
      if (oldValue) {
        for (propName in oldValue) {
          if (!newValue || !(propName in newValue)) {
            setStyle(element.style, propName, "");
          }
        }
      }
      if (newValue) {
        for (propName in newValue) {
          if (!oldValue || newValue[propName] !== oldValue[propName]) {
            setStyle(element.style, propName, newValue[propName]);
          }
        }
      }
    }
  } else if (propName[0] === "o" && propName[1] === "n") {
    isCapture =
      propName !==
      (propName = propName.replace(/(PointerCapture)$|Capture$/i, "$1"));
    if (
      propName.toLowerCase() in element ||
      propName === "onFocusOut" ||
      propName === "onFocusIn"
    ) {
      propName = propName.toLowerCase().slice(2);
    } else {
      propName = propName.slice(2);
    }
    element._listeners ||= {};
    element._listeners[propName + isCapture] = newValue;
    if (newValue) {
      if (oldValue) {
        newValue._attached = oldValue._attached;
      } else {
        newValue._attached = eventClock;
        element.addEventListener(
          propName,
          isCapture ? eventProxyCapture : eventProxy,
          isCapture,
        );
      }
    } else {
      element.removeEventListener(
        propName,
        isCapture ? eventProxyCapture : eventProxy,
        isCapture,
      );
    }
  } else {
    if (namespace == "http://www.w3.org/2000/svg") {
      propName = propName.replace(/xlink(H|:h)/, "h").replace(/sName$/, "s");
    } else if (
      propName != "width" &&
      propName != "height" &&
      propName != "href" &&
      propName != "list" &&
      propName != "form" &&
      propName != "tabIndex" &&
      propName != "download" &&
      propName != "rowSpan" &&
      propName != "colSpan" &&
      propName != "role" &&
      propName != "popover" &&
      propName in element
    ) {
      try {
        element[propName] = newValue == null ? "" : newValue;
        break e;
      } catch (e) {}
    }
    if (typeof newValue != "function") {
      if (newValue == null || (newValue === false && propName[4] !== "-")) {
        element.removeAttribute(propName);
      } else {
        element.setAttribute(
          propName,
          propName == "popover" && newValue == 1 ? "" : newValue,
        );
      }
    }
  }
}
function createEventProxy(eventTypeSuffix) {
  return function (eventObj) {
    if (this._listeners) {
      const eventHandler = this._listeners[eventObj.type + eventTypeSuffix];
      if (eventObj._dispatched == null) {
        eventObj._dispatched = eventClock++;
      } else if (eventObj._dispatched < eventHandler._attached) {
        return;
      }
      return eventHandler(
        options$1.event ? options$1.event(eventObj) : eventObj,
      );
    }
  };
}
const eventProxy = createEventProxy(false);
const eventProxyCapture = createEventProxy(true);
function diff(
  parentDom,
  virtualNode,
  prevVnode,
  globalContext,
  namespaceURI,
  childDomNodes,
  componentCallbackQueue,
  nextSiblingNode,
  isHydrated,
  isHydrating,
) {
  let contextOrRender;
  let componentType = virtualNode.type;
  if (virtualNode.constructor !== undefined) {
    return null;
  }
  if (prevVnode._flags & 128) {
    isHydrated = !!(prevVnode._flags & 32);
    childDomNodes = [(nextSiblingNode = virtualNode._dom = prevVnode._dom)];
  }
  if ((contextOrRender = options$1._diff)) {
    contextOrRender(virtualNode);
  }
  e: if (typeof componentType == "function") {
    try {
      let componentObj;
      let isDirty;
      let prevProps;
      let prevState;
      let snapshotBeforeUpdate;
      let processingException;
      let props = virtualNode.props;
      const hasRenderMethod =
        "prototype" in componentType && componentType.prototype.render;
      contextOrRender = componentType.contextType;
      let contextValueObj =
        contextOrRender && globalContext[contextOrRender._id];
      let currentContext = contextOrRender
        ? contextValueObj
          ? contextValueObj.props.value
          : contextOrRender._defaultValue
        : globalContext;
      if (prevVnode._component) {
        componentObj = virtualNode._component = prevVnode._component;
        processingException = componentObj._processingException =
          componentObj._pendingError;
      } else {
        if (hasRenderMethod) {
          virtualNode._component = componentObj = new componentType(
            props,
            currentContext,
          );
        } else {
          virtualNode._component = componentObj = new BaseComponent(
            props,
            currentContext,
          );
          componentObj.constructor = componentType;
          componentObj.render = doRender;
        }
        if (contextValueObj) {
          contextValueObj.sub(componentObj);
        }
        componentObj.props = props;
        componentObj.state ||= {};
        componentObj.context = currentContext;
        componentObj._globalContext = globalContext;
        isDirty = componentObj._dirty = true;
        componentObj._renderCallbacks = [];
        componentObj._stateCallbacks = [];
      }
      if (hasRenderMethod && componentObj._nextState == null) {
        componentObj._nextState = componentObj.state;
      }
      if (hasRenderMethod && componentType.getDerivedStateFromProps != null) {
        if (componentObj._nextState == componentObj.state) {
          componentObj._nextState = assign({}, componentObj._nextState);
        }
        assign(
          componentObj._nextState,
          componentType.getDerivedStateFromProps(
            props,
            componentObj._nextState,
          ),
        );
      }
      prevProps = componentObj.props;
      prevState = componentObj.state;
      componentObj._vnode = virtualNode;
      if (isDirty) {
        if (
          hasRenderMethod &&
          componentType.getDerivedStateFromProps == null &&
          componentObj.componentWillMount != null
        ) {
          componentObj.componentWillMount();
        }
        if (hasRenderMethod && componentObj.componentDidMount != null) {
          componentObj._renderCallbacks.push(componentObj.componentDidMount);
        }
      } else {
        if (
          hasRenderMethod &&
          componentType.getDerivedStateFromProps == null &&
          props !== prevProps &&
          componentObj.componentWillReceiveProps != null
        ) {
          componentObj.componentWillReceiveProps(props, currentContext);
        }
        if (
          !componentObj._force &&
          ((componentObj.shouldComponentUpdate != null &&
            componentObj.shouldComponentUpdate(
              props,
              componentObj._nextState,
              currentContext,
            ) === false) ||
            virtualNode._original === prevVnode._original)
        ) {
          if (virtualNode._original !== prevVnode._original) {
            componentObj.props = props;
            componentObj.state = componentObj._nextState;
            componentObj._dirty = false;
          }
          virtualNode._dom = prevVnode._dom;
          virtualNode._children = prevVnode._children;
          virtualNode._children.some((e) => {
            if (e) {
              e._parent = virtualNode;
            }
          });
          for (let e = 0; e < componentObj._stateCallbacks.length; e++) {
            componentObj._renderCallbacks.push(componentObj._stateCallbacks[e]);
          }
          componentObj._stateCallbacks = [];
          if (componentObj._renderCallbacks.length) {
            componentCallbackQueue.push(componentObj);
          }
          break e;
        }
        if (componentObj.componentWillUpdate != null) {
          componentObj.componentWillUpdate(
            props,
            componentObj._nextState,
            currentContext,
          );
        }
        if (hasRenderMethod && componentObj.componentDidUpdate != null) {
          componentObj._renderCallbacks.push(() => {
            componentObj.componentDidUpdate(
              prevProps,
              prevState,
              snapshotBeforeUpdate,
            );
          });
        }
      }
      componentObj.context = currentContext;
      componentObj.props = props;
      componentObj._parentDom = parentDom;
      componentObj._force = false;
      let renderHookFn = options$1._render;
      let renderLoopCount = 0;
      if (hasRenderMethod) {
        componentObj.state = componentObj._nextState;
        componentObj._dirty = false;
        if (renderHookFn) {
          renderHookFn(virtualNode);
        }
        contextOrRender = componentObj.render(
          componentObj.props,
          componentObj.state,
          componentObj.context,
        );
        for (let e = 0; e < componentObj._stateCallbacks.length; e++) {
          componentObj._renderCallbacks.push(componentObj._stateCallbacks[e]);
        }
        componentObj._stateCallbacks = [];
      } else {
        do {
          componentObj._dirty = false;
          if (renderHookFn) {
            renderHookFn(virtualNode);
          }
          contextOrRender = componentObj.render(
            componentObj.props,
            componentObj.state,
            componentObj.context,
          );
          componentObj.state = componentObj._nextState;
        } while (componentObj._dirty && ++renderLoopCount < 25);
      }
      componentObj.state = componentObj._nextState;
      if (componentObj.getChildContext != null) {
        globalContext = assign(
          assign({}, globalContext),
          componentObj.getChildContext(),
        );
      }
      if (
        hasRenderMethod &&
        !isDirty &&
        componentObj.getSnapshotBeforeUpdate != null
      ) {
        snapshotBeforeUpdate = componentObj.getSnapshotBeforeUpdate(
          prevProps,
          prevState,
        );
      }
      let renderedOutput =
        contextOrRender != null &&
        contextOrRender.type === Fragment &&
        contextOrRender.key == null
          ? contextOrRender.props.children
          : contextOrRender;
      diffChildren(
        parentDom,
        isArray(renderedOutput) ? renderedOutput : [renderedOutput],
        virtualNode,
        prevVnode,
        globalContext,
        namespaceURI,
        childDomNodes,
        componentCallbackQueue,
        nextSiblingNode,
        isHydrated,
        isHydrating,
      );
      componentObj.base = virtualNode._dom;
      virtualNode._flags &= -161;
      if (componentObj._renderCallbacks.length) {
        componentCallbackQueue.push(componentObj);
      }
      if (processingException) {
        componentObj._pendingError = componentObj._processingException = null;
      }
    } catch (e) {
      virtualNode._original = null;
      if (isHydrated || childDomNodes != null) {
        for (
          virtualNode._flags |= isHydrated ? 160 : 128;
          nextSiblingNode &&
          nextSiblingNode.nodeType === 8 &&
          nextSiblingNode.nextSibling;

        ) {
          nextSiblingNode = nextSiblingNode.nextSibling;
        }
        childDomNodes[childDomNodes.indexOf(nextSiblingNode)] = null;
        virtualNode._dom = nextSiblingNode;
      } else {
        virtualNode._dom = prevVnode._dom;
        virtualNode._children = prevVnode._children;
      }
      options$1._catchError(e, virtualNode, prevVnode);
    }
  } else if (
    childDomNodes == null &&
    virtualNode._original === prevVnode._original
  ) {
    virtualNode._children = prevVnode._children;
    virtualNode._dom = prevVnode._dom;
  } else {
    virtualNode._dom = diffElementNodes(
      prevVnode._dom,
      virtualNode,
      prevVnode,
      globalContext,
      namespaceURI,
      childDomNodes,
      componentCallbackQueue,
      isHydrated,
      isHydrating,
    );
  }
  if ((contextOrRender = options$1.diffed)) {
    contextOrRender(virtualNode);
  }
}
function commitRoot(components, component, refData) {
  component._nextDom = undefined;
  for (let e = 0; e < refData.length; e++) {
    applyRef(refData[e], refData[++e], refData[++e]);
  }
  if (options$1._commit) {
    options$1._commit(component, components);
  }
  components.some((componentInstance) => {
    try {
      components = componentInstance._renderCallbacks;
      componentInstance._renderCallbacks = [];
      components.some((callback) => {
        callback.call(componentInstance);
      });
    } catch (callback) {
      options$1._catchError(callback, componentInstance._vnode);
    }
  });
}
function diffElementNodes(
  domElement,
  virtualNode,
  parentVnode,
  mode,
  namespace,
  existingChildNodes,
  siblingIndex,
  isHydrating,
  prevChildren,
) {
  let key;
  let dangerouslySetInnerHTMLSpec;
  let existingDangerouslySetInnerHTML;
  let childrenProp;
  let tempValue;
  let valueProp;
  let checkedProp;
  let parentProps = parentVnode.props;
  let virtualNodeProps = virtualNode.props;
  let virtualNodeType = virtualNode.type;
  if (virtualNodeType === "svg") {
    namespace = "http://www.w3.org/2000/svg";
  } else if (virtualNodeType === "math") {
    namespace = "http://www.w3.org/1998/Math/MathML";
  } else {
    namespace ||= "http://www.w3.org/1999/xhtml";
  }
  if (existingChildNodes != null) {
    for (key = 0; key < existingChildNodes.length; key++) {
      tempValue = existingChildNodes[key];
      if (
        tempValue &&
        "setAttribute" in tempValue == !!virtualNodeType &&
        (virtualNodeType
          ? tempValue.localName === virtualNodeType
          : tempValue.nodeType === 3)
      ) {
        domElement = tempValue;
        existingChildNodes[key] = null;
        break;
      }
    }
  }
  if (domElement == null) {
    if (virtualNodeType === null) {
      return document.createTextNode(virtualNodeProps);
    }
    domElement = document.createElementNS(
      namespace,
      virtualNodeType,
      virtualNodeProps.is && virtualNodeProps,
    );
    if (isHydrating) {
      if (options$1._hydrationMismatch) {
        options$1._hydrationMismatch(virtualNode, existingChildNodes);
      }
      isHydrating = false;
    }
    existingChildNodes = null;
  }
  if (virtualNodeType === null) {
    if (
      parentProps !== virtualNodeProps &&
      (!isHydrating || domElement.data !== virtualNodeProps)
    ) {
      domElement.data = virtualNodeProps;
    }
  } else {
    existingChildNodes =
      existingChildNodes && slice.call(domElement.childNodes);
    parentProps = parentVnode.props || EMPTY_OBJ;
    if (!isHydrating && existingChildNodes != null) {
      parentProps = {};
      key = 0;
      for (; key < domElement.attributes.length; key++) {
        tempValue = domElement.attributes[key];
        parentProps[tempValue.name] = tempValue.value;
      }
    }
    for (key in parentProps) {
      tempValue = parentProps[key];
      if (key == "children") {
      } else if (key == "dangerouslySetInnerHTML") {
        existingDangerouslySetInnerHTML = tempValue;
      } else if (!(key in virtualNodeProps)) {
        if (
          (key == "value" && "defaultValue" in virtualNodeProps) ||
          (key == "checked" && "defaultChecked" in virtualNodeProps)
        ) {
          continue;
        }
        setProperty(domElement, key, null, tempValue, namespace);
      }
    }
    for (key in virtualNodeProps) {
      tempValue = virtualNodeProps[key];
      if (key == "children") {
        childrenProp = tempValue;
      } else if (key == "dangerouslySetInnerHTML") {
        dangerouslySetInnerHTMLSpec = tempValue;
      } else if (key == "value") {
        valueProp = tempValue;
      } else if (key == "checked") {
        checkedProp = tempValue;
      } else if (
        (!isHydrating || typeof tempValue == "function") &&
        parentProps[key] !== tempValue
      ) {
        setProperty(domElement, key, tempValue, parentProps[key], namespace);
      }
    }
    if (dangerouslySetInnerHTMLSpec) {
      if (
        !isHydrating &&
        (!existingDangerouslySetInnerHTML ||
          (dangerouslySetInnerHTMLSpec.__html !==
            existingDangerouslySetInnerHTML.__html &&
            dangerouslySetInnerHTMLSpec.__html !== domElement.innerHTML))
      ) {
        domElement.innerHTML = dangerouslySetInnerHTMLSpec.__html;
      }
      virtualNode._children = [];
    } else {
      if (existingDangerouslySetInnerHTML) {
        domElement.innerHTML = "";
      }
      diffChildren(
        domElement,
        isArray(childrenProp) ? childrenProp : [childrenProp],
        virtualNode,
        parentVnode,
        mode,
        virtualNodeType === "foreignObject"
          ? "http://www.w3.org/1999/xhtml"
          : namespace,
        existingChildNodes,
        siblingIndex,
        existingChildNodes
          ? existingChildNodes[0]
          : parentVnode._children && getDomSibling(parentVnode, 0),
        isHydrating,
        prevChildren,
      );
      if (existingChildNodes != null) {
        for (key = existingChildNodes.length; key--; ) {
          removeNode(existingChildNodes[key]);
        }
      }
    }
    if (!isHydrating) {
      key = "value";
      if (virtualNodeType === "progress" && valueProp == null) {
        domElement.removeAttribute("value");
      } else if (
        valueProp !== undefined &&
        (valueProp !== domElement[key] ||
          (virtualNodeType === "progress" && !valueProp) ||
          (virtualNodeType === "option" && valueProp !== parentProps[key]))
      ) {
        setProperty(domElement, key, valueProp, parentProps[key], namespace);
      }
      key = "checked";
      if (checkedProp !== undefined && checkedProp !== domElement[key]) {
        setProperty(domElement, key, checkedProp, parentProps[key], namespace);
      }
    }
  }
  return domElement;
}
function applyRef(refObj, value, component) {
  try {
    if (typeof refObj == "function") {
      let t = typeof refObj._unmount == "function";
      if (t) {
        refObj._unmount();
      }
      if (!t || value != null) {
        refObj._unmount = refObj(value);
      }
    } else {
      refObj.current = value;
    }
  } catch (e) {
    options$1._catchError(e, component);
  }
}
function unmount(node, parentComponent, skipRemoval) {
  let entity;
  if (options$1.unmount) {
    options$1.unmount(node);
  }
  if ((entity = node.ref)) {
    if (!entity.current || entity.current === node._dom) {
      applyRef(entity, null, parentComponent);
    }
  }
  if ((entity = node._component) != null) {
    if (entity.componentWillUnmount) {
      try {
        entity.componentWillUnmount();
      } catch (e) {
        options$1._catchError(e, parentComponent);
      }
    }
    entity.base = entity._parentDom = null;
  }
  if ((entity = node._children)) {
    for (let index = 0; index < entity.length; index++) {
      if (entity[index]) {
        unmount(
          entity[index],
          parentComponent,
          skipRemoval || typeof node.type != "function",
        );
      }
    }
  }
  if (!skipRemoval) {
    removeNode(node._dom);
  }
  node._component = node._parent = node._dom = node._nextDom = undefined;
}
function doRender(vnode, props, context) {
  return this.constructor(vnode, context);
}
function render(rootVnode, containerVnode, hydrateFn) {
  if (options$1._root) {
    options$1._root(rootVnode, containerVnode);
  }
  let isHydrate = typeof hydrateFn == "function";
  let prevChildren = isHydrate
    ? null
    : (hydrateFn && hydrateFn._children) || containerVnode._children;
  let commitQueue = [];
  let refData = [];
  diff(
    containerVnode,
    (rootVnode = ((!isHydrate && hydrateFn) || containerVnode)._children =
      createElement(Fragment, null, [rootVnode])),
    prevChildren || EMPTY_OBJ,
    EMPTY_OBJ,
    containerVnode.namespaceURI,
    !isHydrate && hydrateFn
      ? [hydrateFn]
      : prevChildren
        ? null
        : containerVnode.firstChild
          ? slice.call(containerVnode.childNodes)
          : null,
    commitQueue,
    !isHydrate && hydrateFn
      ? hydrateFn
      : prevChildren
        ? prevChildren._dom
        : containerVnode.firstChild,
    isHydrate,
    refData,
  );
  commitRoot(commitQueue, rootVnode, refData);
}
function hydrate(container, element) {
  render(container, element, hydrate);
}
function cloneElement(originalElement, newProps, childOrChildren) {
  let key;
  let ref;
  let propName;
  let defaultProps;
  let clonedProps = assign({}, originalElement.props);
  if (originalElement.type && originalElement.type.defaultProps) {
    defaultProps = originalElement.type.defaultProps;
  }
  for (propName in newProps) {
    if (propName == "key") {
      key = newProps[propName];
    } else if (propName == "ref") {
      ref = newProps[propName];
    } else if (newProps[propName] === undefined && defaultProps !== undefined) {
      clonedProps[propName] = defaultProps[propName];
    } else {
      clonedProps[propName] = newProps[propName];
    }
  }
  if (arguments.length > 2) {
    clonedProps.children =
      arguments.length > 3 ? slice.call(arguments, 2) : childOrChildren;
  }
  return createVNode(
    originalElement.type,
    clonedProps,
    key || originalElement.key,
    ref || originalElement.ref,
    null,
  );
}
let loopIndex = 0;
function createContext(contextValue, contextKey) {
  const context = {
    _id: (contextKey = "__cC" + loopIndex++),
    _defaultValue: contextValue,
    Consumer: (parentElement, childElement) =>
      parentElement.children(childElement),
    Provider(componentInstance) {
      if (!this.getChildContext) {
        let e = new Set();
        let contextObject = {
          [contextKey]: this,
        };
        this.getChildContext = () => contextObject;
        this.componentWillUnmount = () => {
          e = null;
        };
        this.shouldComponentUpdate = function (prevProps) {
          if (this.props.value !== prevProps.value) {
            e.forEach((component) => {
              component._force = true;
              enqueueRender(component);
            });
          }
        };
        this.sub = (component) => {
          e.add(component);
          let originalUnmount = component.componentWillUnmount;
          component.componentWillUnmount = () => {
            if (e) {
              e.delete(component);
            }
            if (originalUnmount) {
              originalUnmount.call(component);
            }
          };
        };
      }
      return componentInstance.children;
    },
  };
  return (context.Provider._contextRef = context.Consumer.contextType =
    context);
}
let currentIndex;
let currentComponent;
let previousComponent;
let currentHook = 0;
let afterPaintEffects = [];
const options = options$1;
let oldBeforeDiff = options._diff;
let oldBeforeRender = options._render;
let oldAfterDiff = options.diffed;
let oldCommit = options._commit;
let oldBeforeUnmount = options.unmount;
let oldRoot = options._root;
const RAF_TIMEOUT = 100;
let prevRaf;
function getHookState(hookIndex, fallbackHookIndex) {
  if (options._hook) {
    options._hook(
      currentComponent,
      hookIndex,
      currentHook || fallbackHookIndex,
    );
  }
  currentHook = 0;
  const hooksState = (currentComponent.__hooks ||= {
    _list: [],
    _pendingEffects: [],
  });
  if (hookIndex >= hooksState._list.length) {
    hooksState._list.push({});
  }
  return hooksState._list[hookIndex];
}
function useState(initialState) {
  currentHook = 1;
  return useReducer(invokeOrReturn, initialState);
}
function useReducer(reducerFunction, initialState, initFunction) {
  const hookState = getHookState(currentIndex++, 2);
  hookState._reducer = reducerFunction;
  if (
    !hookState._component &&
    ((hookState._value = [
      initFunction
        ? initFunction(initialState)
        : invokeOrReturn(undefined, initialState),
      (inputValue) => {
        const currentValue = hookState._nextValue
          ? hookState._nextValue[0]
          : hookState._value[0];
        const updatedValue = hookState._reducer(currentValue, inputValue);
        if (currentValue !== updatedValue) {
          hookState._nextValue = [updatedValue, hookState._value[1]];
          hookState._component.setState({});
        }
      },
    ]),
    (hookState._component = currentComponent),
    !currentComponent._hasScuFromHooks)
  ) {
    currentComponent._hasScuFromHooks = true;
    let originalShouldComponentUpdate = currentComponent.shouldComponentUpdate;
    const originalComponentWillUpdate = currentComponent.componentWillUpdate;
    function shouldComponentUpdateWithHooks(newProps, oldProps, context) {
      if (!hookState._component.__hooks) {
        return true;
      }
      const activeHooks = hookState._component.__hooks._list.filter(
        (e) => !!e._component,
      );
      const allHooksStable = activeHooks.every((hook) => !hook._nextValue);
      if (allHooksStable) {
        return (
          !originalShouldComponentUpdate ||
          originalShouldComponentUpdate.call(this, newProps, oldProps, context)
        );
      }
      let hasValueChanged = false;
      activeHooks.forEach((hook) => {
        if (hook._nextValue) {
          const prevFirstValue = hook._value[0];
          hook._value = hook._nextValue;
          hook._nextValue = undefined;
          if (prevFirstValue !== hook._value[0]) {
            hasValueChanged = true;
          }
        }
      });
      return (
        (!!hasValueChanged || hookState._component.props !== newProps) &&
        (!originalShouldComponentUpdate ||
          originalShouldComponentUpdate.call(this, newProps, oldProps, context))
      );
    }
    currentComponent.componentWillUpdate = function (
      newProps,
      oldProps,
      context,
    ) {
      if (this._force) {
        let originalShouldComponentUpdateBackup = originalShouldComponentUpdate;
        originalShouldComponentUpdate = undefined;
        shouldComponentUpdateWithHooks(newProps, oldProps, context);
        originalShouldComponentUpdate = originalShouldComponentUpdateBackup;
      }
      if (originalComponentWillUpdate) {
        originalComponentWillUpdate.call(this, newProps, oldProps, context);
      }
    };
    currentComponent.shouldComponentUpdate = shouldComponentUpdateWithHooks;
  }
  return hookState._nextValue || hookState._value;
}
function useEffect(effectFn, deps) {
  const hookState = getHookState(currentIndex++, 3);
  if (!options._skipEffects && argsChanged(hookState._args, deps)) {
    hookState._value = effectFn;
    hookState._pendingArgs = deps;
    currentComponent.__hooks._pendingEffects.push(hookState);
  }
}
function useLayoutEffect(effectFn, deps) {
  const hookState = getHookState(currentIndex++, 4);
  if (!options._skipEffects && argsChanged(hookState._args, deps)) {
    hookState._value = effectFn;
    hookState._pendingArgs = deps;
    currentComponent._renderCallbacks.push(hookState);
  }
}
function useRef(initialValue) {
  currentHook = 5;
  return useMemo(
    () => ({
      current: initialValue,
    }),
    [],
  );
}
function useImperativeHandle(ref, createHandle, deps) {
  currentHook = 6;
  useLayoutEffect(
    () =>
      typeof ref == "function"
        ? (ref(createHandle()), () => ref(null))
        : ref
          ? ((ref.current = createHandle()), () => (ref.current = null))
          : undefined,
    deps == null ? deps : deps.concat(ref),
  );
}
function useMemo(factoryFn, deps) {
  const memoState = getHookState(currentIndex++, 7);
  if (argsChanged(memoState._args, deps)) {
    memoState._value = factoryFn();
    memoState._args = deps;
    memoState._factory = factoryFn;
  }
  return memoState._value;
}
function useCallback(callback, deps) {
  currentHook = 8;
  return useMemo(() => callback, deps);
}
function useContext(context) {
  const providerInstance = currentComponent.context[context._id];
  const hookState = getHookState(currentIndex++, 9);
  hookState._context = context;
  if (providerInstance) {
    if (hookState._value == null) {
      hookState._value = true;
      providerInstance.sub(currentComponent);
    }
    return providerInstance.props.value;
  } else {
    return context._defaultValue;
  }
}
function useDebugValue(value, formatFn) {
  if (options.useDebugValue) {
    options.useDebugValue(formatFn ? formatFn(value) : value);
  }
}
function useErrorBoundary(errorHandler) {
  const errorHookState = getHookState(currentIndex++, 10);
  const errorStateTuple = useState();
  errorHookState._value = errorHandler;
  currentComponent.componentDidCatch ||= (newValue, oldValue) => {
    if (errorHookState._value) {
      errorHookState._value(newValue, oldValue);
    }
    errorStateTuple[1](newValue);
  };
  return [
    errorStateTuple[0],
    () => {
      errorStateTuple[1](undefined);
    },
  ];
}
function useId() {
  const hookState = getHookState(currentIndex++, 11);
  if (!hookState._value) {
    let ancestorVNode = currentComponent._vnode;
    while (
      ancestorVNode !== null &&
      !ancestorVNode._mask &&
      ancestorVNode._parent !== null
    ) {
      ancestorVNode = ancestorVNode._parent;
    }
    let maskArray = (ancestorVNode._mask ||= [0, 0]);
    hookState._value = "P" + maskArray[0] + "-" + maskArray[1]++;
  }
  return hookState._value;
}
function flushAfterPaintEffects() {
  let effectItem;
  while ((effectItem = afterPaintEffects.shift())) {
    if (effectItem._parentDom && effectItem.__hooks) {
      try {
        effectItem.__hooks._pendingEffects.forEach(invokeCleanup);
        effectItem.__hooks._pendingEffects.forEach(invokeEffect);
        effectItem.__hooks._pendingEffects = [];
      } catch (error) {
        effectItem.__hooks._pendingEffects = [];
        options._catchError(error, effectItem._vnode);
      }
    }
  }
}
options._diff = (vnode) => {
  currentComponent = null;
  if (oldBeforeDiff) {
    oldBeforeDiff(vnode);
  }
};
options._root = (existingVNode, newVNode) => {
  if (existingVNode && newVNode._children && newVNode._children._mask) {
    existingVNode._mask = newVNode._children._mask;
  }
  if (oldRoot) {
    oldRoot(existingVNode, newVNode);
  }
};
options._render = (vnode) => {
  if (oldBeforeRender) {
    oldBeforeRender(vnode);
  }
  currentComponent = vnode._component;
  currentIndex = 0;
  const hooks = currentComponent.__hooks;
  if (hooks) {
    if (previousComponent === currentComponent) {
      hooks._pendingEffects = [];
      currentComponent._renderCallbacks = [];
      hooks._list.forEach((hook) => {
        if (hook._nextValue) {
          hook._value = hook._nextValue;
        }
        hook._pendingArgs = hook._nextValue = undefined;
      });
    } else {
      hooks._pendingEffects.forEach(invokeCleanup);
      hooks._pendingEffects.forEach(invokeEffect);
      hooks._pendingEffects = [];
      currentIndex = 0;
    }
  }
  previousComponent = currentComponent;
};
options.diffed = (vnode) => {
  if (oldAfterDiff) {
    oldAfterDiff(vnode);
  }
  const componentInstance = vnode._component;
  if (componentInstance && componentInstance.__hooks) {
    if (componentInstance.__hooks._pendingEffects.length) {
      afterPaint(afterPaintEffects.push(componentInstance));
    }
    componentInstance.__hooks._list.forEach((component) => {
      if (component._pendingArgs) {
        component._args = component._pendingArgs;
      }
      component._pendingArgs = undefined;
    });
  }
  previousComponent = currentComponent = null;
};
options._commit = (componentInstance, effects) => {
  effects.some((component) => {
    try {
      component._renderCallbacks.forEach(invokeCleanup);
      component._renderCallbacks = component._renderCallbacks.filter(
        (effect) => !effect._value || invokeEffect(effect),
      );
    } catch (error) {
      effects.some((componentObj) => {
        componentObj._renderCallbacks &&= [];
      });
      effects = [];
      options._catchError(error, component._vnode);
    }
  });
  if (oldCommit) {
    oldCommit(componentInstance, effects);
  }
};
options.unmount = (vnode) => {
  if (oldBeforeUnmount) {
    oldBeforeUnmount(vnode);
  }
  const component = vnode._component;
  if (component && component.__hooks) {
    let e;
    component.__hooks._list.forEach((n) => {
      try {
        invokeCleanup(n);
      } catch (n) {
        e = n;
      }
    });
    component.__hooks = undefined;
    if (e) {
      options._catchError(e, component._vnode);
    }
  }
};
let HAS_RAF = typeof requestAnimationFrame == "function";
function afterNextFrame(callback) {
  const handleNextFrame = () => {
    clearTimeout(timeoutId);
    if (HAS_RAF) {
      cancelAnimationFrame(animationFrameId);
    }
    setTimeout(callback);
  };
  const timeoutId = setTimeout(handleNextFrame, 100);
  let animationFrameId;
  if (HAS_RAF) {
    animationFrameId = requestAnimationFrame(handleNextFrame);
  }
}
function afterPaint(effectsCount) {
  if (effectsCount === 1 || prevRaf !== options.requestAnimationFrame) {
    prevRaf = options.requestAnimationFrame;
    (prevRaf || afterNextFrame)(flushAfterPaintEffects);
  }
}
function invokeCleanup(component) {
  const prevComponent = currentComponent;
  let cleanupFn = component._cleanup;
  if (typeof cleanupFn == "function") {
    component._cleanup = undefined;
    cleanupFn();
  }
  currentComponent = prevComponent;
}
function invokeEffect(callback) {
  const prevComponent = currentComponent;
  callback._cleanup = callback._value();
  currentComponent = prevComponent;
}
function argsChanged(prevArgs, newArgs) {
  return (
    !prevArgs ||
    prevArgs.length !== newArgs.length ||
    newArgs.some((newArg, index) => newArg !== prevArgs[index])
  );
}
function invokeOrReturn(arg, callbackOrValue) {
  if (typeof callbackOrValue == "function") {
    return callbackOrValue(arg);
  } else {
    return callbackOrValue;
  }
}
export {
  BaseComponent as Component,
  Fragment,
  cloneElement,
  createContext,
  createElement,
  createRef,
  createElement as h,
  hydrate,
  isValidElement,
  options$1 as options,
  render,
  toChildArray,
  useCallback,
  useContext,
  useDebugValue,
  useEffect,
  useErrorBoundary,
  useId,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
};
