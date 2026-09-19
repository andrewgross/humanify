var MODE_HYDRATE = 32;
var MODE_SUSPENDED = 128;
var INSERT_VNODE = 4;
var MATCHED = 2;
var RESET_MODE = ~(MODE_HYDRATE | MODE_SUSPENDED);
var SVG_NAMESPACE = "http://www.w3.org/2000/svg";
var XHTML_NAMESPACE = "http://www.w3.org/1999/xhtml";
var MATH_NAMESPACE = "http://www.w3.org/1998/Math/MathML";
var UNDEFINED = undefined;
var EMPTY_OBJ = {};
var EMPTY_ARR = [];
var IS_NON_DIMENSIONAL = /acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i;
var isArray = Array.isArray;
function assign(e, t) {
  for (let n in t) e[n] = t[n];
  return e;
}
function removeNode(e) {
  if (e && e.parentNode) {
    e.parentNode.removeChild(e);
  }
}
var slice = EMPTY_ARR.slice;
function _catchError(e, t, n, o) {
  let r, l, i;
  for (; t = t._parent;) {
    if ((r = t._component) && !r._processingException) {
      try {
        l = r.constructor;
        if (l && l.getDerivedStateFromError != null) {
          r.setState(l.getDerivedStateFromError(e));
          i = r._dirty;
        }
        if (r.componentDidCatch != null) {
          r.componentDidCatch(e, o || {});
          i = r._dirty;
        }
        if (i) {
          return r._pendingError = r;
        }
      } catch (t) {
        e = t;
      }
    }
  }
  throw e;
}
var options = {
  _catchError: _catchError
};
var options_default = options;
var vnodeId = 0;
function createElement(e, t, n) {
  let o, r, l;
  let i = {};
  for (l in t) if (l == "key") {
    o = t[l];
  } else if (l == "ref") {
    r = t[l];
  } else {
    i[l] = t[l];
  }
  if (arguments.length > 2) {
    i.children = arguments.length > 3 ? slice.call(arguments, 2) : n;
  }
  if (typeof e == "function" && e.defaultProps != null) {
    for (l in e.defaultProps) if (i[l] === UNDEFINED) {
      i[l] = e.defaultProps[l];
    }
  }
  return createVNode(e, i, o, r, null);
}
function createVNode(e, t, n, o, r) {
  const l = {
    type: e,
    props: t,
    key: n,
    ref: o,
    _children: null,
    _parent: null,
    _depth: 0,
    _dom: null,
    _component: null,
    constructor: UNDEFINED,
    _original: r == null ? ++vnodeId : r,
    _index: -1,
    _flags: 0
  };
  if (r == null && options_default.vnode != null) {
    options_default.vnode(l);
  }
  return l;
}
function createRef() {
  return {
    current: null
  };
}
function Fragment(e) {
  return e.children;
}
var isValidElement = e => e != null && e.constructor == UNDEFINED;
function BaseComponent(e, t) {
  this.props = e;
  this.context = t;
}
function getDomSibling(e, t) {
  if (t == null) {
    if (e._parent) {
      return getDomSibling(e._parent, e._index + 1);
    } else {
      return null;
    }
  }
  let n;
  for (; t < e._children.length; t++) {
    n = e._children[t];
    if (n != null && n._dom != null) {
      return n._dom;
    }
  }
  if (typeof e.type == "function") {
    return getDomSibling(e);
  } else {
    return null;
  }
}
function renderComponent(e) {
  let t = e._vnode;
  let n = t._dom;
  let o = [];
  let r = [];
  if (e._parentDom) {
    const l = assign({}, t);
    l._original = t._original + 1;
    if (options_default.vnode) {
      options_default.vnode(l);
    }
    diff(e._parentDom, l, t, e._globalContext, e._parentDom.namespaceURI, t._flags & MODE_HYDRATE ? [n] : null, o, n == null ? getDomSibling(t) : n, !!(t._flags & MODE_HYDRATE), r);
    l._original = t._original;
    l._parent._children[l._index] = l;
    commitRoot(o, l, r);
    if (l._dom != n) {
      updateParentDomPointers(l);
    }
  }
}
function updateParentDomPointers(e) {
  if ((e = e._parent) != null && e._component != null) {
    e._dom = e._component.base = null;
    for (let t = 0; t < e._children.length; t++) {
      let n = e._children[t];
      if (n != null && n._dom != null) {
        e._dom = e._component.base = n._dom;
        break;
      }
    }
    return updateParentDomPointers(e);
  }
}
BaseComponent.prototype.setState = function (e, t) {
  let n;
  if (this._nextState != null && this._nextState !== this.state) {
    n = this._nextState;
  } else {
    n = this._nextState = assign({}, this.state);
  }
  if (typeof e == "function") {
    e = e(assign({}, n), this.props);
  }
  if (e) {
    assign(n, e);
  }
  if (e != null && this._vnode) {
    if (t) {
      this._stateCallbacks.push(t);
    }
    enqueueRender(this);
  }
};
BaseComponent.prototype.forceUpdate = function (e) {
  if (this._vnode) {
    this._force = true;
    if (e) {
      this._renderCallbacks.push(e);
    }
    enqueueRender(this);
  }
};
BaseComponent.prototype.render = Fragment;
var prevDebounce;
var rerenderQueue = [];
var defer = typeof Promise == "function" ? Promise.prototype.then.bind(Promise.resolve()) : setTimeout;
function enqueueRender(e) {
  if (!e._dirty && (e._dirty = true) && rerenderQueue.push(e) && !process._rerenderCount++ || prevDebounce !== options_default.debounceRendering) {
    ((prevDebounce = options_default.debounceRendering) || defer)(process);
  }
}
var depthSort = (e, t) => e._vnode._depth - t._vnode._depth;
function process() {
  let e;
  let t = 1;
  for (; rerenderQueue.length;) {
    if (rerenderQueue.length > t) {
      rerenderQueue.sort(depthSort);
    }
    e = rerenderQueue.shift();
    t = rerenderQueue.length;
    if (e._dirty) {
      renderComponent(e);
    }
  }
  process._rerenderCount = 0;
}
function diffChildren(e, t, n, o, r, l, i, a, s, u, d) {
  let c, _, p, f, h;
  let m = o && o._children || EMPTY_ARR;
  let E = t.length;
  for (s = constructNewChildrenArray(n, t, m, s, E), c = 0; c < E; c++) {
    p = n._children[c];
    if (p == null) {
      continue;
    }
    if (-1 === p._index) {
      _ = EMPTY_OBJ;
    } else {
      _ = m[p._index] || EMPTY_OBJ;
    }
    p._index = c;
    let t = diff(e, p, _, r, l, i, a, s, u, d);
    f = p._dom;
    if (p.ref && _.ref != p.ref) {
      if (_.ref) {
        applyRef(_.ref, null, p);
      }
      d.push(p.ref, p._component || f, p);
    }
    if (h == null && f != null) {
      h = f;
    }
    if (p._flags & INSERT_VNODE || _._children === p._children) {
      s = insert(p, s, e);
    } else if (typeof p.type == "function" && t !== UNDEFINED) {
      s = t;
    } else if (f) {
      s = f.nextSibling;
    }
    p._flags &= ~(INSERT_VNODE | MATCHED);
  }
  n._dom = h;
  return s;
}
function constructNewChildrenArray(e, t, n, o, r) {
  let l, i, a;
  let s = n.length;
  let u = s;
  let d = 0;
  for (e._children = new Array(r), l = 0; l < r; l++) {
    i = t[l];
    if (i == null || typeof i == "boolean" || typeof i == "function") {
      e._children[l] = null;
      continue;
    }
    if (typeof i == "string" || typeof i == "number" || typeof i == "bigint" || i.constructor == String) {
      i = e._children[l] = createVNode(null, i, null, null, null);
    } else if (isArray(i)) {
      i = e._children[l] = createVNode(Fragment, {
        children: i
      }, null, null, null);
    } else if (i.constructor === UNDEFINED && i._depth > 0) {
      i = e._children[l] = createVNode(i.type, i.props, i.key, i.ref ? i.ref : null, i._original);
    } else {
      i = e._children[l] = i;
    }
    const o = l + d;
    i._parent = e;
    i._depth = e._depth + 1;
    const r = i._index = findMatchingIndex(i, n, o, u);
    a = null;
    if (-1 !== r) {
      a = n[r];
      u--;
      if (a) {
        a._flags |= MATCHED;
      }
    }
    if (a == null || a._original === null) {
      if (-1 == r) {
        d--;
      }
      if (typeof i.type != "function") {
        i._flags |= INSERT_VNODE;
      }
    } else if (r != o) {
      if (r == o - 1) {
        d--;
      } else if (r == o + 1) {
        d++;
      } else {
        if (r > o) {
          d--;
        } else {
          d++;
        }
        i._flags |= INSERT_VNODE;
      }
    }
  }
  if (u) {
    for (l = 0; l < s; l++) {
      a = n[l];
      if (a != null && (a._flags & MATCHED) == 0) {
        if (a._dom == o) {
          o = getDomSibling(a);
        }
        unmount(a, a);
      }
    }
  }
  return o;
}
function insert(e, t, n) {
  if (typeof e.type == "function") {
    let o = e._children;
    for (let r = 0; o && r < o.length; r++) {
      if (o[r]) {
        o[r]._parent = e;
        t = insert(o[r], t, n);
      }
    }
    return t;
  }
  if (e._dom != t) {
    if (t && e.type && !n.contains(t)) {
      t = getDomSibling(e);
    }
    n.insertBefore(e._dom, t || null);
    t = e._dom;
  }
  do {
    t = t && t.nextSibling;
  } while (t != null && t.nodeType == 8);
  return t;
}
function toChildArray(e, t) {
  t = t || [];
  if (!(e == null || typeof e == "boolean")) {
    if (isArray(e)) {
      e.some(e => {
        toChildArray(e, t);
      });
    } else {
      t.push(e);
    }
  }
  return t;
}
function findMatchingIndex(e, t, n, o) {
  const r = e.key;
  const l = e.type;
  let i = t[n];
  let a = o > (i != null && (i._flags & MATCHED) == 0 ? 1 : 0);
  if (i === null || i && r == i.key && l === i.type && (i._flags & MATCHED) == 0) {
    return n;
  }
  if (a) {
    let e = n - 1;
    let o = n + 1;
    for (; e >= 0 || o < t.length;) {
      if (e >= 0) {
        i = t[e];
        if (i && (i._flags & MATCHED) == 0 && r == i.key && l === i.type) {
          return e;
        }
        e--;
      }
      if (o < t.length) {
        i = t[o];
        if (i && (i._flags & MATCHED) == 0 && r == i.key && l === i.type) {
          return o;
        }
        o++;
      }
    }
  }
  return -1;
}
function setStyle(e, t, n) {
  if (t[0] == "-") {
    e.setProperty(t, n == null ? "" : n);
  } else if (n == null) {
    e[t] = "";
  } else if (typeof n != "number" || IS_NON_DIMENSIONAL.test(t)) {
    e[t] = n;
  } else {
    e[t] = n + "px";
  }
}
process._rerenderCount = 0;
var CAPTURE_REGEX = /(PointerCapture)$|Capture$/i;
var eventClock = 0;
function setProperty(e, t, n, o, r) {
  let l;
  e: if (t == "style") {
    if (typeof n == "string") {
      e.style.cssText = n;
    } else {
      if (typeof o == "string") {
        e.style.cssText = o = "";
      }
      if (o) {
        for (t in o) if (!(n && t in n)) {
          setStyle(e.style, t, "");
        }
      }
      if (n) {
        for (t in n) if (!(o && n[t] === o[t])) {
          setStyle(e.style, t, n[t]);
        }
      }
    }
  } else if (t[0] == "o" && t[1] == "n") {
    l = t != (t = t.replace(CAPTURE_REGEX, "$1"));
    if (t.toLowerCase() in e || t == "onFocusOut" || t == "onFocusIn") {
      t = t.toLowerCase().slice(2);
    } else {
      t = t.slice(2);
    }
    if (!e._listeners) {
      e._listeners = {};
    }
    e._listeners[t + l] = n;
    if (n) {
      if (o) {
        n._attached = o._attached;
      } else {
        n._attached = eventClock;
        e.addEventListener(t, l ? eventProxyCapture : eventProxy, l);
      }
    } else {
      e.removeEventListener(t, l ? eventProxyCapture : eventProxy, l);
    }
  } else {
    if (r == SVG_NAMESPACE) {
      t = t.replace(/xlink(H|:h)/, "h").replace(/sName$/, "s");
    } else if (t != "width" && t != "height" && t != "href" && t != "list" && t != "form" && t != "tabIndex" && t != "download" && t != "rowSpan" && t != "colSpan" && t != "role" && t != "popover" && t in e) {
      try {
        e[t] = n == null ? "" : n;
        break e;
      } catch (e) {}
    }
    if (!(typeof n == "function")) {
      if (n == null || n === false && t[4] != "-") {
        e.removeAttribute(t);
      } else {
        e.setAttribute(t, t == "popover" && n == 1 ? "" : n);
      }
    }
  }
}
function createEventProxy(e) {
  return function (t) {
    if (this._listeners) {
      const n = this._listeners[t.type + e];
      if (t._dispatched == null) {
        t._dispatched = eventClock++;
      } else if (t._dispatched < n._attached) {
        return;
      }
      return n(options_default.event ? options_default.event(t) : t);
    }
  };
}
var eventProxy = createEventProxy(false);
var eventProxyCapture = createEventProxy(true);
function diff(e, t, n, o, r, l, i, a, s, u) {
  let d;
  let c = t.type;
  if (t.constructor !== UNDEFINED) {
    return null;
  }
  if (n._flags & MODE_SUSPENDED) {
    s = !!(n._flags & MODE_HYDRATE);
    l = [a = t._dom = n._dom];
  }
  if (d = options_default._diff) {
    d(t);
  }
  e: if (typeof c == "function") {
    try {
      let _, p, f, h, m, E;
      let g = t.props;
      const y = "prototype" in c && c.prototype.render;
      d = c.contextType;
      let D = d && o[d._id];
      let C = d ? D ? D.props.value : d._defaultValue : o;
      if (n._component) {
        _ = t._component = n._component;
        E = _._processingException = _._pendingError;
      } else {
        if (y) {
          t._component = _ = new c(g, C);
        } else {
          t._component = _ = new BaseComponent(g, C);
          _.constructor = c;
          _.render = doRender;
        }
        if (D) {
          D.sub(_);
        }
        _.props = g;
        if (!_.state) {
          _.state = {};
        }
        _.context = C;
        _._globalContext = o;
        p = _._dirty = true;
        _._renderCallbacks = [];
        _._stateCallbacks = [];
      }
      if (y && _._nextState == null) {
        _._nextState = _.state;
      }
      if (y && c.getDerivedStateFromProps != null) {
        if (_._nextState == _.state) {
          _._nextState = assign({}, _._nextState);
        }
        assign(_._nextState, c.getDerivedStateFromProps(g, _._nextState));
      }
      f = _.props;
      h = _.state;
      _._vnode = t;
      if (p) {
        if (y && c.getDerivedStateFromProps == null && _.componentWillMount != null) {
          _.componentWillMount();
        }
        if (y && _.componentDidMount != null) {
          _._renderCallbacks.push(_.componentDidMount);
        }
      } else {
        if (y && c.getDerivedStateFromProps == null && g !== f && _.componentWillReceiveProps != null) {
          _.componentWillReceiveProps(g, C);
        }
        if (!_._force && (_.shouldComponentUpdate != null && false === _.shouldComponentUpdate(g, _._nextState, C) || t._original == n._original)) {
          if (t._original != n._original) {
            _.props = g;
            _.state = _._nextState;
            _._dirty = false;
          }
          t._dom = n._dom;
          t._children = n._children;
          t._children.some(e => {
            if (e) {
              e._parent = t;
            }
          });
          for (let e = 0; e < _._stateCallbacks.length; e++) {
            _._renderCallbacks.push(_._stateCallbacks[e]);
          }
          _._stateCallbacks = [];
          if (_._renderCallbacks.length) {
            i.push(_);
          }
          break e;
        }
        if (_.componentWillUpdate != null) {
          _.componentWillUpdate(g, _._nextState, C);
        }
        if (y && _.componentDidUpdate != null) {
          _._renderCallbacks.push(() => {
            _.componentDidUpdate(f, h, m);
          });
        }
      }
      _.context = C;
      _.props = g;
      _._parentDom = e;
      _._force = false;
      let S = options_default._render;
      let N = 0;
      if (y) {
        _.state = _._nextState;
        _._dirty = false;
        if (S) {
          S(t);
        }
        d = _.render(_.props, _.state, _.context);
        for (let e = 0; e < _._stateCallbacks.length; e++) {
          _._renderCallbacks.push(_._stateCallbacks[e]);
        }
        _._stateCallbacks = [];
      } else {
        do {
          _._dirty = false;
          if (S) {
            S(t);
          }
          d = _.render(_.props, _.state, _.context);
          _.state = _._nextState;
        } while (_._dirty && ++N < 25);
      }
      _.state = _._nextState;
      if (_.getChildContext != null) {
        o = assign(assign({}, o), _.getChildContext());
      }
      if (y && !p && _.getSnapshotBeforeUpdate != null) {
        m = _.getSnapshotBeforeUpdate(f, h);
      }
      let v = d != null && d.type === Fragment && d.key == null ? d.props.children : d;
      a = diffChildren(e, isArray(v) ? v : [v], t, n, o, r, l, i, a, s, u);
      _.base = t._dom;
      t._flags &= RESET_MODE;
      if (_._renderCallbacks.length) {
        i.push(_);
      }
      if (E) {
        _._pendingError = _._processingException = null;
      }
    } catch (e) {
      t._original = null;
      if (s || l != null) {
        if (e.then) {
          for (t._flags |= s ? MODE_HYDRATE | MODE_SUSPENDED : MODE_SUSPENDED; a && a.nodeType == 8 && a.nextSibling;) {
            a = a.nextSibling;
          }
          l[l.indexOf(a)] = null;
          t._dom = a;
        } else {
          for (let e = l.length; e--;) {
            removeNode(l[e]);
          }
        }
      } else {
        t._dom = n._dom;
        t._children = n._children;
      }
      options_default._catchError(e, t, n);
    }
  } else if (l == null && t._original == n._original) {
    t._children = n._children;
    t._dom = n._dom;
  } else {
    a = t._dom = diffElementNodes(n._dom, t, n, o, r, l, i, s, u);
  }
  if (d = options_default.diffed) {
    d(t);
  }
  if (t._flags & MODE_SUSPENDED) {
    return undefined;
  } else {
    return a;
  }
}
function commitRoot(e, t, n) {
  for (let e = 0; e < n.length; e++) {
    applyRef(n[e], n[++e], n[++e]);
  }
  if (options_default._commit) {
    options_default._commit(t, e);
  }
  e.some(t => {
    try {
      e = t._renderCallbacks;
      t._renderCallbacks = [];
      e.some(e => {
        e.call(t);
      });
    } catch (e) {
      options_default._catchError(e, t._vnode);
    }
  });
}
function diffElementNodes(e, t, n, o, r, l, i, a, s) {
  let u, d, c, _, p, f, h;
  let m = n.props;
  let E = t.props;
  let g = t.type;
  if (g == "svg") {
    r = SVG_NAMESPACE;
  } else if (g == "math") {
    r = MATH_NAMESPACE;
  } else if (!r) {
    r = XHTML_NAMESPACE;
  }
  if (l != null) {
    for (u = 0; u < l.length; u++) {
      p = l[u];
      if (p && "setAttribute" in p == !!g && (g ? p.localName == g : p.nodeType == 3)) {
        e = p;
        l[u] = null;
        break;
      }
    }
  }
  if (e == null) {
    if (g == null) {
      return document.createTextNode(E);
    }
    e = document.createElementNS(r, g, E.is && E);
    if (a) {
      if (options_default._hydrationMismatch) {
        options_default._hydrationMismatch(t, l);
      }
      a = false;
    }
    l = null;
  }
  if (g === null) {
    if (!(m === E || a && e.data === E)) {
      e.data = E;
    }
  } else {
    l = l && slice.call(e.childNodes);
    m = n.props || EMPTY_OBJ;
    if (!a && l != null) {
      for (m = {}, u = 0; u < e.attributes.length; u++) {
        p = e.attributes[u];
        m[p.name] = p.value;
      }
    }
    for (u in m) {
      p = m[u];
      if (u == "children") {
        ;
      } else if (u == "dangerouslySetInnerHTML") {
        c = p;
      } else if (!(u in E)) {
        if (u == "value" && "defaultValue" in E || u == "checked" && "defaultChecked" in E) {
          continue;
        }
        setProperty(e, u, null, p, r);
      }
    }
    for (u in E) {
      p = E[u];
      if (u == "children") {
        _ = p;
      } else if (u == "dangerouslySetInnerHTML") {
        d = p;
      } else if (u == "value") {
        f = p;
      } else if (u == "checked") {
        h = p;
      } else if (!(a && typeof p != "function" || m[u] === p)) {
        setProperty(e, u, p, m[u], r);
      }
    }
    if (d) {
      if (!(a || c && (d.__html === c.__html || d.__html === e.innerHTML))) {
        e.innerHTML = d.__html;
      }
      t._children = [];
    } else {
      if (c) {
        e.innerHTML = "";
      }
      diffChildren(t.type === "template" ? e.content : e, isArray(_) ? _ : [_], t, n, o, g == "foreignObject" ? XHTML_NAMESPACE : r, l, i, l ? l[0] : n._children && getDomSibling(n, 0), a, s);
      if (l != null) {
        for (u = l.length; u--;) {
          removeNode(l[u]);
        }
      }
    }
    if (!a) {
      u = "value";
      if (g == "progress" && f == null) {
        e.removeAttribute("value");
      } else if (f !== UNDEFINED && (f !== e[u] || g == "progress" && !f || g == "option" && f !== m[u])) {
        setProperty(e, u, f, m[u], r);
      }
      u = "checked";
      if (h !== UNDEFINED && h !== e[u]) {
        setProperty(e, u, h, m[u], r);
      }
    }
  }
  return e;
}
function applyRef(e, t, n) {
  try {
    if (typeof e == "function") {
      let n = typeof e._unmount == "function";
      if (n) {
        e._unmount();
      }
      if (!(n && t == null)) {
        e._unmount = e(t);
      }
    } else {
      e.current = t;
    }
  } catch (e) {
    options_default._catchError(e, n);
  }
}
function unmount(e, t, n) {
  let o;
  if (options_default.unmount) {
    options_default.unmount(e);
  }
  if (o = e.ref) {
    if (!(o.current && o.current !== e._dom)) {
      applyRef(o, null, t);
    }
  }
  if ((o = e._component) != null) {
    if (o.componentWillUnmount) {
      try {
        o.componentWillUnmount();
      } catch (e) {
        options_default._catchError(e, t);
      }
    }
    o.base = o._parentDom = null;
  }
  if (o = e._children) {
    for (let r = 0; r < o.length; r++) {
      if (o[r]) {
        unmount(o[r], t, n || typeof e.type != "function");
      }
    }
  }
  if (!n) {
    removeNode(e._dom);
  }
  e._component = e._parent = e._dom = UNDEFINED;
}
function doRender(e, t, n) {
  return this.constructor(e, n);
}
function render(e, t, n) {
  if (t == document) {
    t = document.documentElement;
  }
  if (options_default._root) {
    options_default._root(e, t);
  }
  let o = typeof n == "function";
  let r = o ? null : n && n._children || t._children;
  let l = [];
  let i = [];
  diff(t, e = (!o && n || t)._children = createElement(Fragment, null, [e]), r || EMPTY_OBJ, EMPTY_OBJ, t.namespaceURI, !o && n ? [n] : r ? null : t.firstChild ? slice.call(t.childNodes) : null, l, !o && n ? n : r ? r._dom : t.firstChild, o, i);
  commitRoot(l, e, i);
}
function hydrate(e, t) {
  render(e, t, hydrate);
}
function cloneElement(e, t, n) {
  let o, r, l, i;
  let a = assign({}, e.props);
  if (e.type && e.type.defaultProps) {
    i = e.type.defaultProps;
  }
  for (l in t) if (l == "key") {
    o = t[l];
  } else if (l == "ref") {
    r = t[l];
  } else if (t[l] === UNDEFINED && i !== UNDEFINED) {
    a[l] = i[l];
  } else {
    a[l] = t[l];
  }
  if (arguments.length > 2) {
    a.children = arguments.length > 3 ? slice.call(arguments, 2) : n;
  }
  return createVNode(e.type, a, o || e.key, r || e.ref, null);
}
var i = 0;
function createContext(e) {
  function t(e) {
    if (!this.getChildContext) {
      let e = new Set();
      let n = {};
      n[t._id] = this;
      this.getChildContext = () => n;
      this.componentWillUnmount = () => {
        e = null;
      };
      this.shouldComponentUpdate = function (t) {
        if (this.props.value !== t.value) {
          e.forEach(e => {
            e._force = true;
            enqueueRender(e);
          });
        }
      };
      this.sub = t => {
        e.add(t);
        let n = t.componentWillUnmount;
        t.componentWillUnmount = () => {
          if (e) {
            e.delete(t);
          }
          if (n) {
            n.call(t);
          }
        };
      };
    }
    return e.children;
  }
  t._id = "__cC" + i++;
  t._defaultValue = e;
  t.Consumer = (e, t) => e.children(t);
  t.Provider = t._contextRef = t.Consumer.contextType = t;
  return t;
}
export { BaseComponent as Component, Fragment, cloneElement, createContext, createElement, createRef, createElement as h, hydrate, isValidElement, options_default as options, render, toChildArray };