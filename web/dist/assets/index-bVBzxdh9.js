var gt=Object.defineProperty;var mt=(e,t,r)=>t in e?gt(e,t,{enumerable:!0,configurable:!0,writable:!0,value:r}):e[t]=r;var Ee=(e,t,r)=>mt(e,typeof t!="symbol"?t+"":t,r);(function(){const t=document.createElement("link").relList;if(t&&t.supports&&t.supports("modulepreload"))return;for(const n of document.querySelectorAll('link[rel="modulepreload"]'))s(n);new MutationObserver(n=>{for(const a of n)if(a.type==="childList")for(const i of a.addedNodes)i.tagName==="LINK"&&i.rel==="modulepreload"&&s(i)}).observe(document,{childList:!0,subtree:!0});function r(n){const a={};return n.integrity&&(a.integrity=n.integrity),n.referrerPolicy&&(a.referrerPolicy=n.referrerPolicy),n.crossOrigin==="use-credentials"?a.credentials="include":n.crossOrigin==="anonymous"?a.credentials="omit":a.credentials="same-origin",a}function s(n){if(n.ep)return;n.ep=!0;const a=r(n);fetch(n.href,a)}})();const ke=[{path:"/",title:"Dashboard",icon:"📊"},{path:"/review",title:"Review Queue",icon:"❓"},{path:"/organize",title:"Organize",icon:"📁"},{path:"/orphans",title:"Orphans",icon:"📝"}];class pt{constructor(){Ee(this,"handlers",[]);window.addEventListener("hashchange",()=>this.handleChange()),window.addEventListener("load",()=>this.handleChange())}handleChange(){const t=this.getCurrentRoute();this.handlers.forEach(r=>r(t))}getCurrentRoute(){const t=window.location.hash.slice(1)||"/";return["/","/review","/organize","/orphans"].includes(t)?t:"/"}navigate(t){window.location.hash=t}onRouteChange(t){this.handlers.push(t)}}const Ne=new pt,bt="";class ft{async request(t,r){const s=await fetch(`${bt}${t}`,{headers:{"Content-Type":"application/json"},...r});if(!s.ok){const n=await s.json().catch(()=>({error:`HTTP ${s.status}: ${s.statusText}`,code:"HTTP_ERROR"}));throw new C(n.error,n.code,s.status)}return s.json()}async getStats(){return this.request("/api/stats")}async getReviewStats(){return this.request("/api/stats/review")}async getOrganizeStats(){return this.request("/api/stats/organize")}async getReviewQueue(t){const r=new URLSearchParams;t!=null&&t.repo&&r.set("repo",t.repo),t!=null&&t.type&&r.set("type",t.type);const s=r.toString();return this.request(`/api/review/queue${s?`?${s}`:""}`)}async getDocumentReview(t){return this.request(`/api/review/queue/${encodeURIComponent(t)}`)}async answerQuestion(t,r,s){return this.request(`/api/review/answer/${encodeURIComponent(t)}`,{method:"POST",body:JSON.stringify({question_index:r,answer:s})})}async bulkAnswerQuestions(t){return this.request("/api/review/bulk-answer",{method:"POST",body:JSON.stringify({answers:t})})}async getReviewStatus(){return this.request("/api/review/status")}async applyAnswers(t){return this.request("/api/apply",{method:"POST",body:JSON.stringify(t??{})})}async triggerScan(t){return this.request("/api/scan",{method:"POST",body:JSON.stringify(t??{})})}async triggerCheck(t){return this.request("/api/check",{method:"POST",body:JSON.stringify(t??{})})}async getSuggestions(t){const r=new URLSearchParams;t!=null&&t.repo&&r.set("repo",t.repo),t!=null&&t.type&&r.set("type",t.type),(t==null?void 0:t.threshold)!==void 0&&r.set("threshold",t.threshold.toString());const s=r.toString();return this.request(`/api/organize/suggestions${s?`?${s}`:""}`)}async getDocumentSuggestions(t){return this.request(`/api/organize/suggestions/${encodeURIComponent(t)}`)}async dismissSuggestion(t,r,s){return this.request("/api/organize/dismiss",{method:"POST",body:JSON.stringify({type:t,doc_id:r,target_id:s})})}async getOrphans(t){return this.request(`/api/organize/orphans?repo=${encodeURIComponent(t)}`)}async assignOrphan(t,r,s){return this.request("/api/organize/assign-orphan",{method:"POST",body:JSON.stringify({repo:t,line_number:r,target:s})})}async getDocument(t,r){const s=new URLSearchParams;(r==null?void 0:r.include_preview)!==void 0&&s.set("include_preview",r.include_preview.toString()),(r==null?void 0:r.max_content_length)!==void 0&&s.set("max_content_length",r.max_content_length.toString());const n=s.toString();return this.request(`/api/documents/${encodeURIComponent(t)}${n?`?${n}`:""}`)}async getDocumentLinks(t){return this.request(`/api/documents/${encodeURIComponent(t)}/links`)}async getRepositories(){return this.request("/api/repos")}}class C extends Error{constructor(t,r,s){super(t),this.code=r,this.status=s,this.name="ApiRequestError"}get isNotFound(){return this.status===404||this.code==="NOT_FOUND"}get isBadRequest(){return this.status===400||this.code==="BAD_REQUEST"}get isServerError(){return this.status>=500}}const k=new ft;function xt(){return`
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 animate-pulse" aria-hidden="true">
      <div class="flex items-center justify-between mb-3">
        <div class="h-5 bg-gray-200 dark:bg-gray-700 rounded w-1/3"></div>
        <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-16"></div>
      </div>
      <div class="space-y-2">
        <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-full"></div>
        <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-2/3"></div>
      </div>
    </div>
  `}function Fe(e=3){return`
    <div class="space-y-4" role="status" aria-label="Loading content">
      <span class="sr-only">Loading content...</span>
      ${Array(e).fill(0).map(()=>xt()).join("")}
    </div>
  `}function ce(){return`
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden animate-pulse" aria-hidden="true">
      <div class="px-4 py-3 bg-gray-50 dark:bg-gray-700 border-b border-gray-200 dark:border-gray-600">
        <div class="flex items-center justify-between">
          <div class="space-y-2">
            <div class="h-5 bg-gray-300 dark:bg-gray-600 rounded w-48"></div>
            <div class="h-3 bg-gray-200 dark:bg-gray-700 rounded w-32"></div>
          </div>
          <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-20"></div>
        </div>
      </div>
      <div class="p-4 space-y-3">
        <div class="flex items-start space-x-3">
          <div class="h-6 w-16 bg-gray-200 dark:bg-gray-700 rounded"></div>
          <div class="flex-1 space-y-2">
            <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-full"></div>
            <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-3/4"></div>
          </div>
        </div>
        <div class="flex items-start space-x-3">
          <div class="h-6 w-16 bg-gray-200 dark:bg-gray-700 rounded"></div>
          <div class="flex-1 space-y-2">
            <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-full"></div>
            <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-1/2"></div>
          </div>
        </div>
      </div>
    </div>
  `}function yt(){return`
    <dl class="grid grid-cols-2 md:grid-cols-4 gap-4 animate-pulse" aria-hidden="true">
      ${Array(4).fill(0).map(()=>`
        <div>
          <div class="h-3 bg-gray-200 dark:bg-gray-700 rounded w-20 mb-2"></div>
          <div class="h-6 bg-gray-300 dark:bg-gray-600 rounded w-12"></div>
        </div>
      `).join("")}
    </dl>
  `}function de(e){const{title:t="Error",message:r,onRetry:s,retryLabel:n="Retry"}=e,a=s?`retry-${Date.now()}`:null;return`
    <div class="text-center py-8">
      <div class="inline-flex items-center justify-center w-12 h-12 rounded-full bg-red-100 dark:bg-red-900/30 mb-4">
        <svg class="w-6 h-6 text-red-600 dark:text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"></path>
        </svg>
      </div>
      <p class="font-medium text-red-600 dark:text-red-400">${ue(t)}</p>
      <p class="text-sm text-gray-600 dark:text-gray-400 mt-1">${ue(r)}</p>
      ${a?`
        <button id="${a}" class="mt-4 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 text-sm transition-colors">
          ${ue(n)}
        </button>
      `:""}
    </div>
  `}function le(e){const t=document.querySelectorAll('[id^="retry-"]'),r=t[t.length-1];r==null||r.addEventListener("click",e)}function ue(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}const z=[];let be="toast-container";function Ue(){if(document.getElementById(be))return;const e=document.createElement("div");e.id=be,e.className="fixed bottom-4 right-4 z-50 flex flex-col space-y-2 max-w-sm",e.setAttribute("role","region"),e.setAttribute("aria-label","Notifications"),document.body.appendChild(e)}function te(e){Ue();const t=`toast-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,r=e.duration??5e3,s={id:t,options:e};return r>0&&(s.timeoutId=window.setTimeout(()=>fe(t),r)),z.push(s),Qe(),t}function fe(e){const t=z.findIndex(s=>s.id===e);if(t===-1)return;const r=z[t];r.timeoutId&&clearTimeout(r.timeoutId),z.splice(t,1),Qe()}const w={success:(e,t)=>te({message:e,type:"success",...t}),error:(e,t)=>te({message:e,type:"error",duration:0,...t}),info:(e,t)=>te({message:e,type:"info",...t}),warning:(e,t)=>te({message:e,type:"warning",...t})};function Qe(){const e=document.getElementById(be);e&&(e.innerHTML=z.map(t=>vt(t)).join(""),z.forEach(t=>{const r=document.getElementById(`${t.id}-dismiss`);if(r==null||r.addEventListener("click",()=>fe(t.id)),t.options.action){const s=document.getElementById(`${t.id}-action`);s==null||s.addEventListener("click",()=>{var n;(n=t.options.action)==null||n.onClick(),fe(t.id)})}}))}function vt(e){const{id:t,options:r}=e,{message:s,type:n="info",action:a}=r,i=ht(n),l=kt(n);return`
    <div
      id="${t}"
      class="flex items-start p-4 rounded-lg shadow-lg ${i.bg} ${i.border} border animate-slide-in"
      role="alert"
      aria-live="polite"
    >
      <div class="flex-shrink-0 ${i.icon}">
        ${l}
      </div>
      <div class="ml-3 flex-1">
        <p class="text-sm font-medium ${i.text}">${Le(s)}</p>
        ${a?`
          <button
            id="${t}-action"
            class="mt-2 text-sm font-medium ${i.action} hover:underline"
          >
            ${Le(a.label)}
          </button>
        `:""}
      </div>
      <button
        id="${t}-dismiss"
        class="ml-4 flex-shrink-0 ${i.dismiss} hover:opacity-75"
        aria-label="Dismiss"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
        </svg>
      </button>
    </div>
  `}function ht(e){switch(e){case"success":return{bg:"bg-green-50 dark:bg-green-900/30",border:"border-green-200 dark:border-green-800",text:"text-green-800 dark:text-green-200",icon:"text-green-500 dark:text-green-400",action:"text-green-700 dark:text-green-300",dismiss:"text-green-500 dark:text-green-400"};case"error":return{bg:"bg-red-50 dark:bg-red-900/30",border:"border-red-200 dark:border-red-800",text:"text-red-800 dark:text-red-200",icon:"text-red-500 dark:text-red-400",action:"text-red-700 dark:text-red-300",dismiss:"text-red-500 dark:text-red-400"};case"warning":return{bg:"bg-amber-50 dark:bg-amber-900/30",border:"border-amber-200 dark:border-amber-800",text:"text-amber-800 dark:text-amber-200",icon:"text-amber-500 dark:text-amber-400",action:"text-amber-700 dark:text-amber-300",dismiss:"text-amber-500 dark:text-amber-400"};case"info":default:return{bg:"bg-blue-50 dark:bg-blue-900/30",border:"border-blue-200 dark:border-blue-800",text:"text-blue-800 dark:text-blue-200",icon:"text-blue-500 dark:text-blue-400",action:"text-blue-700 dark:text-blue-300",dismiss:"text-blue-500 dark:text-blue-400"}}}function kt(e){switch(e){case"success":return`
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
        </svg>
      `;case"error":return`
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
        </svg>
      `;case"warning":return`
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"></path>
        </svg>
      `;case"info":default:return`
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
        </svg>
      `}}function Le(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}const u={stats:null,review:null,organize:null,loading:!0,error:null,autoRefresh:!1,refreshInterval:null},wt=3e4;function $t(e){return e<1024?`${e} B`:e<1024*1024?`${(e/1024).toFixed(1)} KB`:`${(e/(1024*1024)).toFixed(1)} MB`}function Et(e){return e?new Date(e).toLocaleString():"Never"}async function ae(){u.loading=!0,u.error=null,Se();try{const[e,t,r]=await Promise.all([k.getStats(),k.getReviewStats(),k.getOrganizeStats()]);u.stats=e,u.review=t,u.organize=r}catch(e){e instanceof C?u.error=e.message:u.error="Failed to load dashboard data"}finally{u.loading=!1,Se()}}function Se(){var i,l,m;const e=document.getElementById("review-count");e&&(e.textContent=u.loading?"...":((i=u.review)==null?void 0:i.unanswered.toString())??"-");const t=document.getElementById("deferred-count");if(t){const $=((l=u.review)==null?void 0:l.deferred)??0;t.textContent=$>0?`⚠ ${$} deferred`:""}const r=document.getElementById("organize-count");if(r){const $=u.organize?u.organize.merge_candidates+u.organize.misplaced_candidates+u.organize.duplicate_entry_count:0;r.textContent=u.loading?"...":$.toString()}const s=document.getElementById("orphan-count");s&&(s.textContent=u.loading?"...":((m=u.organize)==null?void 0:m.orphan_count.toString())??"-");const n=document.getElementById("stats-content");n&&(u.loading?n.innerHTML=yt():u.error?(n.innerHTML=de({title:"Error loading stats",message:u.error,onRetry:ae}),le(ae)):u.stats&&(n.innerHTML=`
        <dl class="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Repositories</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${u.stats.repos_count}</dd>
          </div>
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Documents</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${u.stats.docs_count}</dd>
          </div>
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Database Size</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${$t(u.stats.db_size_bytes)}</dd>
          </div>
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Last Scan</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${Et(u.stats.last_scan)}</dd>
          </div>
        </dl>
      `));const a=document.getElementById("auto-refresh-toggle");a&&(a.checked=u.autoRefresh)}function Lt(e){u.autoRefresh=e,e&&!u.refreshInterval?u.refreshInterval=window.setInterval(()=>ae(),wt):!e&&u.refreshInterval&&(clearInterval(u.refreshInterval),u.refreshInterval=null)}function Ce(){return`
    <div class="space-y-4 sm:space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Dashboard</h2>
        <div class="flex items-center space-x-3">
          <button id="scan-btn" class="inline-flex items-center px-3 py-2 text-sm font-medium rounded-md bg-green-600 text-white hover:bg-green-700 disabled:opacity-50">🔄 Scan</button>
          <button id="check-btn" class="inline-flex items-center px-3 py-2 text-sm font-medium rounded-md bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50">🔍 Check</button>
          <label class="flex items-center space-x-2 text-sm text-gray-600 dark:text-gray-300">
            <input type="checkbox" id="auto-refresh-toggle" class="rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500" ${u.autoRefresh?"checked":""}>
            <span>Auto-refresh</span>
          </label>
        </div>
      </div>
      <div id="action-result"></div>
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 sm:gap-6">
        <a href="#/review" class="block bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6 hover:shadow-lg transition-shadow">
          <div class="flex items-center space-x-3">
            <span class="text-2xl sm:text-3xl">❓</span>
            <div>
              <h3 class="text-base sm:text-lg font-semibold text-gray-900 dark:text-white">Review Queue</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">Pending questions</p>
            </div>
          </div>
          <div id="review-count" class="mt-3 sm:mt-4 text-2xl sm:text-3xl font-bold text-blue-600 dark:text-blue-400">-</div>
          <div id="deferred-count" class="mt-1 text-sm text-amber-600 dark:text-amber-400"></div>
        </a>
        <a href="#/organize" class="block bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6 hover:shadow-lg transition-shadow">
          <div class="flex items-center space-x-3">
            <span class="text-2xl sm:text-3xl">📁</span>
            <div>
              <h3 class="text-base sm:text-lg font-semibold text-gray-900 dark:text-white">Organize</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">Suggestions</p>
            </div>
          </div>
          <div id="organize-count" class="mt-3 sm:mt-4 text-2xl sm:text-3xl font-bold text-green-600 dark:text-green-400">-</div>
        </a>
        <a href="#/orphans" class="block bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6 hover:shadow-lg transition-shadow sm:col-span-2 lg:col-span-1">
          <div class="flex items-center space-x-3">
            <span class="text-2xl sm:text-3xl">📝</span>
            <div>
              <h3 class="text-base sm:text-lg font-semibold text-gray-900 dark:text-white">Orphans</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">Unassigned facts</p>
            </div>
          </div>
          <div id="orphan-count" class="mt-3 sm:mt-4 text-2xl sm:text-3xl font-bold text-amber-600 dark:text-amber-400">-</div>
        </a>
      </div>
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6">
        <h3 class="text-base sm:text-lg font-semibold text-gray-900 dark:text-white mb-4">Quick Stats</h3>
        <div id="stats-content" class="text-gray-600 dark:text-gray-300">Loading...</div>
      </div>
    </div>
  `}function St(){var t,r;const e=document.getElementById("auto-refresh-toggle");e==null||e.addEventListener("change",s=>{Lt(s.target.checked)}),(t=document.getElementById("scan-btn"))==null||t.addEventListener("click",async s=>{const n=s.currentTarget;n.disabled=!0,n.textContent="⏳ Scanning...";try{const a=await k.triggerScan();a.status==="cli_required"&&(w.info(a.message),Me(a.command))}catch(a){w.error(a instanceof Error?a.message:"Scan failed")}finally{n.disabled=!1,n.textContent="🔄 Scan"}}),(r=document.getElementById("check-btn"))==null||r.addEventListener("click",async s=>{const n=s.currentTarget;n.disabled=!0,n.textContent="⏳ Checking...";try{const a=await k.triggerCheck();a.status==="cli_required"&&(w.info(a.message),Me(a.command))}catch(a){w.error(a instanceof Error?a.message:"Check failed")}finally{n.disabled=!1,n.textContent="🔍 Check"}}),ae()}function Me(e){const t=document.getElementById("action-result");t&&(t.innerHTML=`
      <div class="bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 rounded-lg p-4">
        <p class="text-sm text-blue-800 dark:text-blue-200">Run in terminal:</p>
        <code class="block mt-1 text-sm bg-blue-100 dark:bg-blue-900 px-2 py-1 rounded font-mono">${e}</code>
      </div>
    `)}function Ct(){u.refreshInterval&&(clearInterval(u.refreshInterval),u.refreshInterval=null)}const W=new Map;function We(e,t){return`${e}:${t}`}function Ke(e,t){const r=We(e,t);return W.has(r)||W.set(r,{submitting:!1,error:null}),W.get(r)}function T(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function Mt(e,t,r){const s=Ke(e,t),n=`answer-form-${T(e)}-${t}`,a=`answer-input-${T(e)}-${t}`,i=`answer-label-${T(e)}-${t}`,l=`answer-hint-${T(e)}-${t}`,m=Tt(r),$=s.submitting?"opacity-50 pointer-events-none":"";return`
    <form id="${n}" class="answer-form mt-3 ${$}" data-doc-id="${T(e)}" data-question-index="${t}">
      <div class="space-y-2">
        <label id="${i}" for="${a}" class="sr-only">Answer for ${T(r)} question</label>
        <textarea
          id="${a}"
          name="answer"
          rows="2"
          class="block w-full rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm resize-none"
          placeholder="${T(m)}"
          aria-labelledby="${i}"
          aria-describedby="${l}"
          ${s.submitting?'disabled aria-busy="true"':""}
        ></textarea>
        <div id="${l}" class="sr-only">Press Ctrl+Enter to submit. Use Dismiss to skip or Delete fact to remove.</div>
        <div id="answer-hint-live-${T(e)}-${t}" class="text-xs text-gray-400 dark:text-gray-500 h-4" aria-live="polite"></div>
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-2">
            <button
              type="submit"
              class="inline-flex items-center px-3 py-1.5 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-50"
              ${s.submitting?'disabled aria-busy="true"':""}
            >
              ${s.submitting?`
                <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" fill="none" viewBox="0 0 24 24" aria-hidden="true">
                  <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                  <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                Submitting...
              `:"Submit"}
            </button>
            <span class="text-xs text-gray-400 dark:text-gray-500" aria-hidden="true">Ctrl+Enter</span>
          </div>
          <div class="flex items-center space-x-2">
            <button
              type="button"
              class="quick-action inline-flex items-center px-2 py-1 text-xs font-medium text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 rounded"
              data-action="dismiss"
              aria-label="Dismiss this question"
              ${s.submitting?"disabled":""}
            >
              Dismiss
            </button>
            <button
              type="button"
              class="quick-action inline-flex items-center px-2 py-1 text-xs font-medium text-red-600 dark:text-red-400 hover:text-red-900 dark:hover:text-red-200 hover:bg-red-50 dark:hover:bg-red-900/20 rounded"
              data-action="delete"
              aria-label="Delete the referenced fact"
              ${s.submitting?"disabled":""}
            >
              Delete fact
            </button>
          </div>
        </div>
        ${s.error?`
          <div class="text-sm text-red-600 dark:text-red-400" role="alert">${T(s.error)}</div>
        `:""}
      </div>
    </form>
  `}function Tt(e){switch(e){case"temporal":return'e.g., "Started March 2022, left December 2024"';case"conflict":return'e.g., "Both were part-time, no conflict" or explain resolution';case"missing":return'e.g., "LinkedIn profile, checked 2024-01-15"';case"ambiguous":return'e.g., "Home address" or "split: home in Austin, work in SF"';case"stale":return'e.g., "Still accurate as of today" or provide update';case"duplicate":return'e.g., "Keep this one" or "Merge into [other_id]"';default:return"Enter your answer..."}}async function ge(e,t,r,s){const n=Ke(e,t);n.submitting=!0,n.error=null;try{await k.answerQuestion(e,t,r),s.onSuccess(e,t,r),W.delete(We(e,t))}catch(a){a instanceof C?n.error=a.message:n.error="Failed to submit answer",s.onError(n.error)}finally{n.submitting=!1}}function It(e,t){e.addEventListener("submit",async r=>{const s=r.target.closest(".answer-form");if(!s)return;r.preventDefault();const n=s.dataset.docId,a=parseInt(s.dataset.questionIndex||"0",10),i=s.querySelector("textarea"),l=(i==null?void 0:i.value.trim())||"";!n||!l||await ge(n,a,l,t)}),e.addEventListener("click",async r=>{const s=r.target.closest(".quick-action");if(!s)return;const n=s.closest(".answer-form");if(!n)return;const a=n.dataset.docId,i=parseInt(n.dataset.questionIndex||"0",10),l=s.dataset.action;if(!a||!l)return;await ge(a,i,l==="dismiss"?"dismiss":"delete",t)}),e.addEventListener("input",r=>{const s=r.target;if(s.tagName!=="TEXTAREA")return;const n=s.closest(".answer-form");if(!n)return;const a=n.dataset.docId,i=n.dataset.questionIndex;if(!a||!i)return;const l=document.getElementById(`answer-hint-live-${a}-${i}`);l&&(l.textContent=At(s.value.trim()))}),e.addEventListener("keydown",async r=>{if(r.key==="Enter"&&(r.ctrlKey||r.metaKey)){const s=r.target;if(s.tagName!=="TEXTAREA")return;const n=s.closest(".answer-form");if(!n)return;r.preventDefault();const a=n.dataset.docId,i=parseInt(n.dataset.questionIndex||"0",10),l=s.value.trim();if(!a||!l)return;await ge(a,i,l,t)}})}function At(e){if(!e)return"";const t=e.toLowerCase();return t==="dismiss"||t==="ignore"?"→ Will dismiss this question":t==="delete"||t==="remove"?"→ Will delete the referenced fact":t==="defer"||t.startsWith("defer ")||t.startsWith("needs ")?"→ Will defer for later review":/^(yes|confirmed|still accurate|correct)$/i.test(t)||t.startsWith("yes")&&e.length<30?"→ Will refresh last-seen date (@t[~])":/\d{4}[-/]\d{2}/.test(e)||/^(per |via |from |according to )/i.test(t)?"→ Looks like a source citation — will add footnote":t.startsWith("correct:")||t.startsWith("correction:")?"→ Will rewrite the fact with LLM assistance":""}function _t(){W.clear()}const g={selections:new Set,submitting:!1,showBulkAnswer:!1};function we(e,t){return`${e}:${t}`}function Ge(e){const[t,r]=e.split(":");return{docId:t,questionIndex:parseInt(r,10)}}function me(){return Array.from(g.selections).map(Ge)}function qt(e,t){const r=we(e,t);g.selections.has(r)?g.selections.delete(r):g.selections.add(r)}function Bt(e){g.selections.clear();for(const t of e)for(let r=0;r<t.questions.length;r++)t.questions[r].answered||g.selections.add(we(t.doc_id,r))}function Rt(){g.selections.clear()}function Je(){g.selections.clear(),g.submitting=!1,g.showBulkAnswer=!1}function Te(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function jt(e){const t=g.selections.size,r=t>0;return`
    <div id="bulk-actions-bar" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4" role="toolbar" aria-label="Bulk actions">
      <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3">
        <div class="flex items-center space-x-4">
          <span class="text-sm text-gray-600 dark:text-gray-400" aria-live="polite">
            <span id="bulk-selected-count" class="font-medium">${t}</span> of ${e} selected
          </span>
          <div class="flex items-center space-x-2" role="group" aria-label="Selection controls">
            <button
              id="bulk-select-all"
              class="text-sm text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-200"
              aria-label="Select all unanswered questions"
            >
              Select all
            </button>
            <span class="text-gray-300 dark:text-gray-600" aria-hidden="true">|</span>
            <button
              id="bulk-select-none"
              class="text-sm text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-200"
              aria-label="Clear selection"
            >
              Select none
            </button>
          </div>
        </div>
        <div class="flex items-center space-x-2" role="group" aria-label="Bulk actions">
          <button
            id="bulk-dismiss-btn"
            class="inline-flex items-center px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md disabled:opacity-50 disabled:cursor-not-allowed"
            ${!r||g.submitting?'disabled aria-disabled="true"':""}
            aria-label="Dismiss ${t} selected questions"
          >
            Dismiss selected
          </button>
          <button
            id="bulk-answer-btn"
            class="inline-flex items-center px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md disabled:opacity-50 disabled:cursor-not-allowed"
            ${!r||g.submitting?'disabled aria-disabled="true"':""}
            ${g.submitting?'aria-busy="true"':""}
            aria-label="Answer ${t} selected questions"
          >
            ${g.submitting?`
              <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" fill="none" viewBox="0 0 24 24" aria-hidden="true">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Processing...
            `:"Answer selected..."}
          </button>
        </div>
      </div>
      ${g.showBulkAnswer?Ht():""}
    </div>
  `}function Ht(){const e="bulk-answer-input",t="bulk-answer-label";return`
    <div id="bulk-answer-form" class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700">
      <div class="space-y-3">
        <label id="${t}" for="${e}" class="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Apply same answer to ${g.selections.size} selected question(s)
        </label>
        <textarea
          id="${e}"
          rows="2"
          class="block w-full rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm resize-none"
          placeholder="Enter answer to apply to all selected questions..."
          aria-labelledby="${t}"
          ${g.submitting?'disabled aria-busy="true"':""}
        ></textarea>
        <div class="flex items-center justify-end space-x-2">
          <button
            id="bulk-answer-cancel"
            class="px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md"
            ${g.submitting?"disabled":""}
          >
            Cancel
          </button>
          <button
            id="bulk-answer-submit"
            class="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md disabled:opacity-50"
            ${g.submitting?'disabled aria-busy="true"':""}
          >
            Apply to all
          </button>
        </div>
      </div>
    </div>
  `}function Dt(e,t,r){if(r.answered)return"";const s=we(e,t),n=g.selections.has(s);return`
    <input
      type="checkbox"
      id="${`bulk-checkbox-${Te(e)}-${t}`}"
      class="bulk-checkbox h-4 w-4 rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500"
      data-doc-id="${Te(e)}"
      data-question-index="${t}"
      aria-label="Select question ${t+1} for bulk action"
      ${n?"checked":""}
    />
  `}async function Ie(e,t,r){if(g.selections.size!==0){g.submitting=!0;try{const s=Array.from(g.selections).map(i=>{const{docId:l,questionIndex:m}=Ge(i);return{doc_id:l,question_index:m,answer:t}}),n=await k.bulkAnswerQuestions(s);n.errors&&n.errors.length>0&&r.onError(`Some answers failed: ${n.errors.join(", ")}`);const a=n.results.filter(i=>i.success).length;a>0&&(r.onSuccess(a,e==="dismiss"?"dismissed":"answered"),g.selections.clear(),g.showBulkAnswer=!1)}catch(s){s instanceof C?r.onError(s.message):r.onError("Failed to process bulk action")}finally{g.submitting=!1}}}function zt(e,t,r,s){var n,a,i,l,m,$;(n=e.querySelector("#bulk-select-all"))==null||n.addEventListener("click",()=>{Bt(t),r.onSelectionChange(me()),s()}),(a=e.querySelector("#bulk-select-none"))==null||a.addEventListener("click",()=>{Rt(),r.onSelectionChange(me()),s()}),(i=e.querySelector("#bulk-dismiss-btn"))==null||i.addEventListener("click",async()=>{if(g.selections.size===0)return;const E=g.selections.size;confirm(`Dismiss ${E} selected question(s)?`)&&(await Ie("dismiss","dismiss",r),s())}),(l=e.querySelector("#bulk-answer-btn"))==null||l.addEventListener("click",()=>{g.submitting||(g.showBulkAnswer=!g.showBulkAnswer,s())}),(m=e.querySelector("#bulk-answer-cancel"))==null||m.addEventListener("click",()=>{g.showBulkAnswer=!1,s()}),($=e.querySelector("#bulk-answer-submit"))==null||$.addEventListener("click",async()=>{const E=e.querySelector("#bulk-answer-input"),M=E==null?void 0:E.value.trim();M&&(await Ie("answer",M,r),s())}),e.addEventListener("change",E=>{const M=E.target;if(!M.classList.contains("bulk-checkbox"))return;const j=M.dataset.docId,ut=parseInt(M.dataset.questionIndex||"0",10);j&&(qt(j,ut),r.onSelectionChange(me()),s())})}const Pt={temporal:{bg:"bg-blue-100 dark:bg-blue-900",text:"text-blue-700 dark:text-blue-200"},conflict:{bg:"bg-red-100 dark:bg-red-900",text:"text-red-700 dark:text-red-200"},missing:{bg:"bg-amber-100 dark:bg-amber-900",text:"text-amber-700 dark:text-amber-200"},ambiguous:{bg:"bg-purple-100 dark:bg-purple-900",text:"text-purple-700 dark:text-purple-200"},stale:{bg:"bg-gray-100 dark:bg-gray-700",text:"text-gray-700 dark:text-gray-200"},duplicate:{bg:"bg-green-100 dark:bg-green-900",text:"text-green-700 dark:text-green-200"}};function Q(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function Ve(e){const t=Pt[e]||{bg:"bg-gray-100 dark:bg-gray-700",text:"text-gray-700 dark:text-gray-200"};return`<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${t.bg} ${t.text}">@q[${Q(e)}]</span>`}function Ot(e,t,r,s={}){const{showAnswerForm:n=!0,showCheckbox:a=!1}=s,i=e.answered?"opacity-60":"",l=e.answered?'<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-200 ml-2">Answered</span>':"",m=a?Dt(t,r,e):"",$=e.line_ref?`<button
        class="preview-line-btn text-xs text-blue-600 dark:text-blue-400 hover:underline"
        data-doc-id="${Q(t)}"
        data-line-ref="${e.line_ref}"
      >Line ${e.line_ref}</button>`:"",E=e.answered&&e.answer?`<div class="mt-2 p-2 bg-gray-50 dark:bg-gray-800 rounded text-sm text-gray-600 dark:text-gray-400">
        <span class="font-medium">Answer:</span> ${Q(e.answer)}
      </div>`:n&&!e.answered?Mt(t,r,e.question_type):"";return`
    <div class="question-card border border-gray-200 dark:border-gray-700 rounded-lg p-4 ${i}" data-doc-id="${Q(t)}" data-question-index="${r}">
      <div class="flex items-start justify-between">
        <div class="flex items-center space-x-2">
          ${m}
          ${Ve(e.question_type)}
          ${l}
          ${$}
        </div>
      </div>
      <p class="mt-2 text-gray-700 dark:text-gray-300">${Q(e.description)}</p>
      ${E}
    </div>
  `}const h={loading:!1,error:null,document:null,highlightLine:null};let K=!1,p=null;function q(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function Ae(e){return!e||e.length===0?'<p class="text-sm text-gray-500 dark:text-gray-400">None</p>':`
    <ul class="space-y-1">
      ${e.map(t=>`
        <li>
          <button
            class="preview-link-btn text-sm text-blue-600 dark:text-blue-400 hover:underline text-left"
            data-doc-id="${q(t.id)}"
          >
            ${q(t.title)}
          </button>
        </li>
      `).join("")}
    </ul>
  `}function Nt(e,t){return e.split(`
`).map((s,n)=>{const a=n+1,i=t!==null&&a===t;return`
      <div class="flex ${i?"bg-yellow-100 dark:bg-yellow-900/50 border-l-4 border-yellow-400":""}" ${i?'id="highlighted-line"':""}>
        <span class="select-none w-10 flex-shrink-0 text-right pr-3 ${i?"text-yellow-600 dark:text-yellow-400 font-bold":"text-gray-400 dark:text-gray-600"} text-xs leading-6">${a}</span>
        <pre class="flex-1 text-sm leading-6 whitespace-pre-wrap break-words text-gray-800 dark:text-gray-200">${q(s)||" "}</pre>
      </div>
    `}).join("")}function Xe(){if(h.loading)return`
      <div class="flex items-center justify-center h-64" role="status" aria-live="polite">
        <div class="text-center">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600" aria-hidden="true"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading document...</p>
        </div>
      </div>
    `;if(h.error)return`
      <div class="p-4 text-center" role="alert">
        <p class="text-red-600 dark:text-red-400">${q(h.error)}</p>
        <button id="preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;if(!h.document)return'<div class="p-4 text-gray-500 dark:text-gray-400">No document selected</div>';const e=h.document;return`
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-start justify-between">
          <div class="flex-1 min-w-0">
            <h3 class="text-lg font-semibold text-gray-900 dark:text-white truncate" id="preview-title">${q(e.title)}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 truncate">${q(e.file_path)}</p>
            <div class="mt-1 flex items-center space-x-2">
              <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300">
                ${q(e.doc_type)}
              </span>
              <span class="text-xs text-gray-500 dark:text-gray-400">${q(e.id)}</span>
            </div>
          </div>
          <button id="preview-close-btn" class="ml-2 p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200" aria-label="Close preview">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
            </svg>
          </button>
        </div>
      </div>

      <!-- Links section -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="grid grid-cols-2 gap-4">
          <div>
            <h4 class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2" id="links-to-heading">Links to</h4>
            <nav aria-labelledby="links-to-heading">
              ${Ae(e.links_to||[])}
            </nav>
          </div>
          <div>
            <h4 class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2" id="linked-from-heading">Linked from</h4>
            <nav aria-labelledby="linked-from-heading">
              ${Ae(e.linked_from||[])}
            </nav>
          </div>
        </div>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-auto p-4 font-mono bg-white dark:bg-gray-900" role="region" aria-label="Document content">
        ${e.content?Nt(e.content,h.highlightLine):'<p class="text-gray-500 dark:text-gray-400">No content available</p>'}
      </div>
    </div>
  `}function _e(){if(!p)return;const e=p.querySelector("#preview-panel-content");e&&(e.innerHTML=Xe(),Ye(),h.highlightLine!==null&&!h.loading&&setTimeout(()=>{const t=document.getElementById("highlighted-line");t==null||t.scrollIntoView({behavior:"smooth",block:"center"})},100))}function Ye(){var e,t;(e=document.getElementById("preview-close-btn"))==null||e.addEventListener("click",xe),(t=document.getElementById("preview-close-error"))==null||t.addEventListener("click",xe),document.querySelectorAll(".preview-link-btn").forEach(r=>{r.addEventListener("click",s=>{const n=s.currentTarget.dataset.docId;n&&G(n)})})}async function Ft(e){h.loading=!0,h.error=null,h.document=null,_e();try{const t=await k.getDocument(e),r=await k.getDocumentLinks(e);h.document={...t,links_to:r.links_to,linked_from:r.linked_from}}catch(t){t instanceof C?h.error=t.message:h.error="Failed to load document"}finally{h.loading=!1,_e()}}function G(e,t){h.highlightLine=t??null,K||(Ut(),K=!0),Ft(e)}function xe(){p&&(p.classList.add("translate-x-full"),setTimeout(()=>{p==null||p.remove(),p=null},300)),K=!1,h.document=null,h.error=null,h.highlightLine=null}function Ut(){p==null||p.remove(),p=document.createElement("div"),p.id="document-preview-panel",p.className=`
    fixed top-0 right-0 h-full w-full sm:w-[480px] lg:w-[560px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g," "),p.setAttribute("role","dialog"),p.setAttribute("aria-modal","true"),p.setAttribute("aria-labelledby","preview-title"),p.innerHTML=`
    <div id="preview-panel-content" class="h-full flex flex-col">
      ${Xe()}
    </div>
  `,document.body.appendChild(p),requestAnimationFrame(()=>{p==null||p.classList.remove("translate-x-full")}),Ye();const e=t=>{t.key==="Escape"&&K&&xe()};document.addEventListener("keydown",e),p._cleanup=()=>{document.removeEventListener("keydown",e)}}function $e(){if(p){const e=p._cleanup;e&&e(),p.remove(),p=null}K=!1,h.document=null,h.error=null,h.highlightLine=null}const d={data:null,repos:[],loading:!0,error:null,filterRepo:"",filterType:"",successMessage:null,bulkMode:!1},Qt=["temporal","conflict","missing","ambiguous","stale","duplicate"];function P(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}async function B(){d.loading=!0,d.error=null,R();try{const e={};d.filterRepo&&(e.repo=d.filterRepo),d.filterType&&(e.type=d.filterType);const[t,r]=await Promise.all([k.getReviewQueue(e),d.repos.length===0?k.getRepositories():Promise.resolve({repositories:d.repos})]);d.data=t,d.repos=r.repositories}catch(e){e instanceof C?d.error=e.message:d.error="Failed to load review queue"}finally{d.loading=!1,R()}}function Wt(e){const t=e.questions.filter(a=>!a.answered).length,r=e.questions.length,n=e.file_path.includes("/archive/")?'<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-500 dark:text-gray-400 ml-2" title="Archived documents are excluded from checks">📦 archived</span>':"";return`
    <div class="document-group bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden">
      <div class="px-4 py-3 bg-gray-50 dark:bg-gray-700 border-b border-gray-200 dark:border-gray-600">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-medium text-gray-900 dark:text-white">${P(e.doc_title)}${n}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400">${P(e.file_path)}</p>
          </div>
          <div class="flex items-center space-x-3">
            <button
              class="preview-doc-btn text-sm text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 flex items-center space-x-1"
              data-doc-id="${P(e.doc_id)}"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path>
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"></path>
              </svg>
              <span>Preview</span>
            </button>
            <div class="text-sm text-gray-500 dark:text-gray-400">
              <span class="font-medium">${t}</span> / ${r} pending
            </div>
          </div>
        </div>
      </div>
      <div class="p-4 space-y-3">
        ${e.questions.map((a,i)=>Ot(a,e.doc_id,i,{showAnswerForm:!d.bulkMode,showCheckbox:d.bulkMode})).join("")}
      </div>
    </div>
  `}function Kt(){const e=d.repos.map(r=>`<option value="${P(r.id)}" ${d.filterRepo===r.id?"selected":""}>${P(r.name)}</option>`).join(""),t=Qt.map(r=>`<option value="${r}" ${d.filterType===r?"selected":""}>${r}</option>`).join("");return`
    <div class="flex flex-col sm:flex-row gap-4">
      <div class="flex-1 sm:flex-none">
        <label for="filter-repo" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Repository</label>
        <select id="filter-repo" class="block w-full sm:w-48 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
          <option value="">All repositories</option>
          ${e}
        </select>
      </div>
      <div class="flex-1 sm:flex-none">
        <label for="filter-type" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Question Type</label>
        <select id="filter-type" class="block w-full sm:w-48 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
          <option value="">All types</option>
          ${t}
        </select>
      </div>
    </div>
  `}function Ze(){if(!d.data)return"";const e={};for(const r of d.data.documents)for(const s of r.questions)s.answered||(e[s.question_type]=(e[s.question_type]||0)+1);return`
    <div class="flex items-center justify-between text-sm">
      <div class="flex items-center space-x-1">${Object.entries(e).sort((r,s)=>s[1]-r[1]).map(([r,s])=>`${Ve(r)} <span class="ml-1 text-gray-600 dark:text-gray-400">${s}</span>`).join('<span class="mx-2 text-gray-300 dark:text-gray-600">|</span>')||'<span class="text-gray-500 dark:text-gray-400">No pending questions</span>'}</div>
      <div class="text-gray-500 dark:text-gray-400">
        ${d.data.unanswered} pending / ${d.data.total} total
      </div>
    </div>
  `}function R(){const e=document.getElementById("review-queue-content");if(!e)return;const t=document.getElementById("review-summary");t&&d.data&&(t.innerHTML=Ze());const r=document.getElementById("workflow-stepper");r&&(r.innerHTML=et());const s=document.getElementById("deferred-banner");s&&(s.innerHTML=tt());const n=document.getElementById("apply-bar");n&&(n.innerHTML=rt(),st());const a=document.getElementById("bulk-actions-container");a&&d.data&&d.bulkMode?(a.innerHTML=jt(d.data.unanswered),zt(a,d.data.documents,{onSuccess:Yt,onError:Zt,onSelectionChange:er},R)):a&&(a.innerHTML="");const i=document.getElementById("review-message");if(i&&(i.innerHTML=""),d.loading){e.innerHTML=`
      <div class="space-y-4">
        ${ce()}
        ${ce()}
        ${ce()}
      </div>
    `;return}if(d.error){e.innerHTML=de({title:"Error loading review queue",message:d.error,onRetry:B}),le(B);return}if(!d.data||d.data.documents.length===0){e.innerHTML=`
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No pending review questions</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Run <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase lint --review</code> to generate questions</p>
      </div>
    `;return}const l=[...d.data.documents].sort((m,$)=>{const E=m.questions.filter(j=>!j.answered).length;return $.questions.filter(j=>!j.answered).length-E});e.innerHTML=`
    <div class="space-y-4">
      ${l.map(m=>Wt(m)).join("")}
    </div>
  `,d.bulkMode||It(e,{onSuccess:Vt,onError:Xt}),Gt(e)}function Gt(e){e.querySelectorAll(".preview-doc-btn").forEach(t=>{t.addEventListener("click",r=>{const s=r.currentTarget.dataset.docId;s&&G(s)})}),e.querySelectorAll(".preview-line-btn").forEach(t=>{t.addEventListener("click",r=>{const s=r.currentTarget.dataset.docId,n=r.currentTarget.dataset.lineRef;s&&G(s,n?parseInt(n,10):void 0)})})}function Jt(){const e=document.getElementById("filter-repo"),t=document.getElementById("filter-type");e==null||e.addEventListener("change",r=>{d.filterRepo=r.target.value,B()}),t==null||t.addEventListener("change",r=>{d.filterType=r.target.value,B()})}function Vt(e,t,r){if(d.data){const s=d.data.documents.find(n=>n.doc_id===e);s&&s.questions[t]&&(s.questions[t].answered=!0,s.questions[t].answer=r,d.data.answered++,d.data.unanswered--)}w.success(`Answer submitted for question ${t+1}`),R()}function Xt(e){w.error(`Failed to submit answer: ${e}`),R()}function Yt(e,t){w.success(`Successfully ${t} ${e} question(s)`),B()}function Zt(e){w.error(`Bulk action failed: ${e}`),R()}function er(e){}function tr(){d.bulkMode=!d.bulkMode,d.bulkMode||Je(),R()}function et(){if(!d.data)return"";const e=d.data.unanswered,t=d.data.answered;let r=1;return e===0&&t>0?r=3:t>0?r=2:e===0&&t===0&&(r=4),`
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
      <div class="flex items-center justify-between text-sm">
        ${[{num:1,label:"Review questions"},{num:2,label:"Answer/defer"},{num:3,label:"Apply answers"},{num:4,label:"Verify"}].map(n=>{const a=n.num===r,i=n.num<r,l=a?"text-blue-600 dark:text-blue-400 font-semibold":i?"text-green-600 dark:text-green-400":"text-gray-400 dark:text-gray-500",m=i?"✓":n.num.toString();return`<div class="flex items-center space-x-1 ${l}"><span class="w-5 h-5 flex items-center justify-center rounded-full ${a?"bg-blue-100 dark:bg-blue-900":i?"bg-green-100 dark:bg-green-900":"bg-gray-100 dark:bg-gray-700"} text-xs">${m}</span><span class="hidden sm:inline">${n.label}</span></div>`}).join('<div class="flex-1 h-px bg-gray-200 dark:bg-gray-700 mx-2"></div>')}
      </div>
    </div>
  `}function tt(){if(!d.data)return"";let e=0;for(const t of d.data.documents)for(const r of t.questions)!r.answered&&r.answer&&(r.answer.toLowerCase().startsWith("defer")||r.answer.toLowerCase().startsWith("needs "))&&e++;return e===0?"":`
    <div class="bg-amber-50 dark:bg-amber-900/30 border border-amber-200 dark:border-amber-800 rounded-lg p-4">
      <div class="flex items-center justify-between">
        <div class="flex items-center space-x-2">
          <span class="text-amber-600 dark:text-amber-400">⚠</span>
          <span class="text-sm font-medium text-amber-800 dark:text-amber-200">${e} item${e!==1?"s":""} need${e===1?"s":""} human attention</span>
        </div>
        <button id="filter-deferred-btn" class="text-sm text-amber-700 dark:text-amber-300 hover:underline">Show deferred</button>
      </div>
    </div>
  `}function rt(){return!d.data||d.data.answered===0?"":`
    <div class="bg-green-50 dark:bg-green-900/30 border border-green-200 dark:border-green-800 rounded-lg p-4">
      <div class="flex items-center justify-between">
        <span class="text-sm text-green-800 dark:text-green-200">${d.data.answered} answered question${d.data.answered!==1?"s":""} ready to apply</span>
        <div class="flex items-center space-x-2">
          <button id="apply-preview-btn" class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-green-100 dark:bg-green-800 text-green-700 dark:text-green-200 hover:bg-green-200 dark:hover:bg-green-700">Preview</button>
          <button id="apply-btn" class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-green-600 text-white hover:bg-green-700 disabled:opacity-50">Apply Answers</button>
        </div>
      </div>
      <div id="apply-result" class="mt-2"></div>
    </div>
  `}async function qe(e){const t=document.getElementById(e?"apply-preview-btn":"apply-btn");t&&(t.disabled=!0,t.textContent="⏳ ...");try{const r=await k.applyAnswers({dry_run:e}),s=document.getElementById("apply-result");if(s)if(r.total_applied===0)s.innerHTML=`<p class="text-sm text-gray-600 dark:text-gray-400">${r.message}</p>`;else{const n=r.documents.map(a=>`<li class="text-sm"><span class="font-medium">${P(a.doc_title)}</span>: ${a.questions_applied??0} question${(a.questions_applied??0)!==1?"s":""} ${a.status}</li>`).join("");s.innerHTML=`<p class="text-sm font-medium mb-1">${r.message}</p><ul class="list-disc list-inside space-y-1">${n}</ul>`}!e&&r.total_applied>0&&(w.success(`Applied ${r.total_applied} answer(s)`),B())}catch(r){r instanceof C&&r.status===503?w.info(r.message):w.error(r instanceof Error?r.message:"Apply failed")}finally{t&&(t.disabled=!1,t.textContent=e?"Preview":"Apply Answers")}}function rr(){const e=d.bulkMode?"Exit bulk mode":"Bulk actions";return`
    <div class="space-y-4 sm:space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Review Queue</h2>
        <button
          id="bulk-mode-toggle"
          class="inline-flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md ${d.bulkMode?"bg-blue-600 text-white hover:bg-blue-700":"bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600"}"
        >
          ${e}
        </button>
      </div>
      <div id="workflow-stepper">${et()}</div>
      <div id="deferred-banner">${tt()}</div>
      <div id="apply-bar">${rt()}</div>
      <div id="review-message"></div>
      <div id="bulk-actions-container"></div>
      <div id="review-filters" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${Kt()}
      </div>
      <div id="review-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${Ze()}
      </div>
      <div id="review-queue-content">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading review queue...</p>
        </div>
      </div>
    </div>
  `}function sr(){Jt(),nr(),st(),B()}function st(){var e,t,r;(e=document.getElementById("apply-preview-btn"))==null||e.addEventListener("click",()=>qe(!0)),(t=document.getElementById("apply-btn"))==null||t.addEventListener("click",()=>qe(!1)),(r=document.getElementById("filter-deferred-btn"))==null||r.addEventListener("click",()=>{d.filterType="",w.info("Deferred items are shown with ⚠ markers in the queue")})}function nr(){var e;(e=document.getElementById("bulk-mode-toggle"))==null||e.addEventListener("click",tr)}function ar(){d.data=null,d.loading=!0,d.error=null,d.successMessage=null,d.bulkMode=!1,_t(),Je(),$e()}function v(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function I(e){const t={merge:"bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200",misplaced:"bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200",duplicate:"bg-rose-100 text-rose-800 dark:bg-rose-900 dark:text-rose-200"},r={merge:"🔗",misplaced:"📁",duplicate:"👥"};return`<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${t[e]}">
    <span class="mr-1">${r[e]}</span>${e}
  </span>`}function ir(e){return`${Math.round(e*100)}%`}function or(e){return e>=.95?"text-red-600 dark:text-red-400":e>=.9?"text-amber-600 dark:text-amber-400":"text-gray-600 dark:text-gray-400"}function dr(e,t,r={}){const{showDismiss:s=!0,showApprove:n=!0,showCompare:a=!0}=r,i=or(e.similarity);return`
    <div class="suggestion-card merge-card bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4" data-type="merge" data-index="${t}">
      <div class="flex items-start justify-between">
        <div class="flex-1">
          <div class="flex items-center space-x-2 mb-2">
            ${I("merge")}
            <span class="text-sm font-medium ${i}">
              ${ir(e.similarity)} similar
            </span>
          </div>
          <div class="space-y-2">
            <div class="flex items-center space-x-2">
              <span class="text-gray-500 dark:text-gray-400 text-sm">Doc 1:</span>
              <button
                class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
                data-doc-id="${v(e.doc1_id)}"
              >
                ${v(e.doc1_title)}
              </button>
              <span class="text-gray-400 dark:text-gray-500 text-xs">[${v(e.doc1_id)}]</span>
            </div>
            <div class="flex items-center space-x-2">
              <span class="text-gray-500 dark:text-gray-400 text-sm">Doc 2:</span>
              <button
                class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
                data-doc-id="${v(e.doc2_id)}"
              >
                ${v(e.doc2_title)}
              </button>
              <span class="text-gray-400 dark:text-gray-500 text-xs">[${v(e.doc2_id)}]</span>
            </div>
          </div>
          <p class="mt-2 text-sm text-gray-600 dark:text-gray-300">
            These documents have high content similarity and may be candidates for merging.
          </p>
        </div>
        ${s||n||a?`
        <div class="flex items-center space-x-2 ml-4">
          ${a?`
          <button
            class="compare-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-doc1="${v(e.doc1_id)}"
            data-doc2="${v(e.doc2_id)}"
            title="Compare documents side-by-side"
          >
            Compare
          </button>
          `:""}
          ${n?`
          <button
            class="approve-btn px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            data-type="merge"
            data-doc1="${v(e.doc1_id)}"
            data-doc2="${v(e.doc2_id)}"
            title="Approve merge (requires CLI)"
          >
            Approve
          </button>
          `:""}
          ${s?`
          <button
            class="dismiss-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-type="merge"
            data-doc-id="${v(e.doc1_id)}"
            data-target-id="${v(e.doc2_id)}"
          >
            Dismiss
          </button>
          `:""}
        </div>
        `:""}
      </div>
    </div>
  `}function lr(e,t,r={}){const{showDismiss:s=!0,showApprove:n=!0,showSections:a=!0}=r;return`
    <div class="suggestion-card misplaced-card bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4" data-type="misplaced" data-index="${t}">
      <div class="flex items-start justify-between">
        <div class="flex-1">
          <div class="flex items-center space-x-2 mb-2">
            ${I("misplaced")}
          </div>
          <div class="flex items-center space-x-2 mb-2">
            <button
              class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
              data-doc-id="${v(e.doc_id)}"
            >
              ${v(e.doc_title)}
            </button>
            <span class="text-gray-400 dark:text-gray-500 text-xs">[${v(e.doc_id)}]</span>
          </div>
          <div class="flex items-center space-x-2 text-sm">
            <span class="text-gray-500 dark:text-gray-400">Current type:</span>
            <span class="px-2 py-0.5 bg-gray-100 dark:bg-gray-700 rounded text-gray-700 dark:text-gray-300">${v(e.current_type)}</span>
            <span class="text-gray-400 dark:text-gray-500">→</span>
            <span class="text-gray-500 dark:text-gray-400">Suggested:</span>
            <span class="px-2 py-0.5 bg-green-100 dark:bg-green-900 rounded text-green-700 dark:text-green-300">${v(e.suggested_type)}</span>
          </div>
          <p class="mt-2 text-sm text-gray-600 dark:text-gray-300">
            ${v(e.reason)}
          </p>
        </div>
        ${s||n||a?`
        <div class="flex items-center space-x-2 ml-4">
          ${a?`
          <button
            class="sections-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-doc-id="${v(e.doc_id)}"
            title="View document sections"
          >
            Sections
          </button>
          `:""}
          ${n?`
          <button
            class="approve-btn px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            data-type="misplaced"
            data-doc-id="${v(e.doc_id)}"
            data-suggested-type="${v(e.suggested_type)}"
            title="Approve retype (requires CLI)"
          >
            Approve
          </button>
          `:""}
          ${s?`
          <button
            class="dismiss-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-type="misplaced"
            data-doc-id="${v(e.doc_id)}"
          >
            Dismiss
          </button>
          `:""}
        </div>
        `:""}
      </div>
    </div>
  `}const y={loading:!1,error:null,doc1:null,doc2:null};let J=!1,f=null,X=null,Y=null;function _(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function ye(e){const t=e.split(`
`);let r=0;for(const s of t){const n=s.trim();(n.startsWith("- ")||n.startsWith("* ")||/^\d+\.\s/.test(n))&&r++}return r}function Be(e,t){var n;if(!e)return`
      <div class="flex-1 min-w-0 p-4 bg-gray-50 dark:bg-gray-800/50 rounded-lg">
        <div class="text-center text-gray-500 dark:text-gray-400">
          Loading...
        </div>
      </div>
    `;const r=e.content?ye(e.content):0,s=((n=e.content)==null?void 0:n.split(`
`))||[];return`
    <div class="flex-1 min-w-0 flex flex-col bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <!-- Header -->
      <div class="flex-shrink-0 p-3 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">${t}</span>
          <span class="text-xs text-gray-500 dark:text-gray-400">${r} fact${r!==1?"s":""}</span>
        </div>
        <h4 class="mt-1 text-sm font-semibold text-gray-900 dark:text-white truncate" title="${_(e.title)}">
          ${_(e.title)}
        </h4>
        <div class="mt-1 flex items-center space-x-2">
          <span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
            ${_(e.doc_type)}
          </span>
          <span class="text-xs text-gray-400 dark:text-gray-500">[${_(e.id)}]</span>
        </div>
        <p class="mt-1 text-xs text-gray-500 dark:text-gray-400 truncate" title="${_(e.file_path)}">
          ${_(e.file_path)}
        </p>
      </div>
      <!-- Content -->
      <div class="flex-1 overflow-auto p-3 font-mono text-xs">
        ${s.map((a,i)=>`
          <div class="flex hover:bg-gray-50 dark:hover:bg-gray-700/50">
            <span class="select-none w-8 flex-shrink-0 text-right pr-2 text-gray-400 dark:text-gray-600">${i+1}</span>
            <pre class="flex-1 whitespace-pre-wrap break-words text-gray-700 dark:text-gray-300">${_(a)||" "}</pre>
          </div>
        `).join("")}
      </div>
    </div>
  `}function nt(){var s,n;if(y.loading)return`
      <div class="flex items-center justify-center h-64">
        <div class="text-center">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading documents...</p>
        </div>
      </div>
    `;if(y.error)return`
      <div class="p-4 text-center">
        <p class="text-red-600 dark:text-red-400">${_(y.error)}</p>
        <button id="merge-preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;const e=(s=y.doc1)!=null&&s.content?ye(y.doc1.content):0,t=(n=y.doc2)!=null&&n.content?ye(y.doc2.content):0;return`
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-semibold text-gray-900 dark:text-white">Merge Preview</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400">Compare documents before merging</p>
          </div>
          <button id="merge-preview-close-btn" class="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
            </svg>
          </button>
        </div>
      </div>

      <!-- Summary -->
      <div class="flex-shrink-0 p-4 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-center justify-between text-sm">
          <div class="flex items-center space-x-4">
            <span class="text-gray-600 dark:text-gray-300">
              <span class="font-medium">${e+t}</span> total facts
            </span>
            <span class="text-gray-400 dark:text-gray-500">|</span>
            <span class="text-gray-600 dark:text-gray-300">
              Doc 1: <span class="font-medium">${e}</span>
            </span>
            <span class="text-gray-600 dark:text-gray-300">
              Doc 2: <span class="font-medium">${t}</span>
            </span>
          </div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            Merged document will contain facts from both
          </div>
        </div>
      </div>

      <!-- Side-by-side comparison -->
      <div class="flex-1 overflow-hidden p-4">
        <div class="flex gap-4 h-full">
          ${Be(y.doc1,"Document 1")}
          ${Be(y.doc2,"Document 2")}
        </div>
      </div>

      <!-- Actions -->
      <div class="flex-shrink-0 p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <p class="text-xs text-gray-500 dark:text-gray-400">
            Merge requires CLI: <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize merge ${X||"doc1"} ${Y||"doc2"}</code>
          </p>
          <div class="flex items-center space-x-2">
            <button
              id="merge-preview-dismiss-btn"
              class="px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            >
              Dismiss
            </button>
            <button
              id="merge-preview-approve-btn"
              class="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            >
              Copy Command
            </button>
          </div>
        </div>
      </div>
    </div>
  `}function Re(){if(!f)return;const e=f.querySelector("#merge-preview-content");e&&(e.innerHTML=nt(),at())}function at(){var e,t,r,s;(e=document.getElementById("merge-preview-close-btn"))==null||e.addEventListener("click",se),(t=document.getElementById("merge-preview-close-error"))==null||t.addEventListener("click",se),(r=document.getElementById("merge-preview-dismiss-btn"))==null||r.addEventListener("click",()=>{se()}),(s=document.getElementById("merge-preview-approve-btn"))==null||s.addEventListener("click",()=>{const n=`factbase organize merge ${X} ${Y}`;navigator.clipboard.writeText(n).then(()=>{const a=document.getElementById("merge-preview-approve-btn");a&&(a.textContent="Copied!",setTimeout(()=>{a.textContent="Copy Command"},2e3))}).catch(()=>{alert(`Run: ${n}`)})})}async function cr(e,t){y.loading=!0,y.error=null,y.doc1=null,y.doc2=null,Re();try{const[r,s]=await Promise.all([k.getDocument(e),k.getDocument(t)]);y.doc1=r,y.doc2=s}catch(r){r instanceof C?y.error=r.message:y.error="Failed to load documents"}finally{y.loading=!1,Re()}}function ur(e,t){X=e,Y=t,J||(gr(),J=!0),cr(e,t)}function se(){f&&(f.classList.add("translate-x-full"),setTimeout(()=>{f==null||f.remove(),f=null},300)),J=!1,y.doc1=null,y.doc2=null,y.error=null,X=null,Y=null}function gr(){f==null||f.remove(),f=document.createElement("div"),f.id="merge-preview-panel",f.className=`
    fixed top-0 right-0 h-full w-full lg:w-[900px] xl:w-[1100px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g," "),f.innerHTML=`
    <div id="merge-preview-content" class="h-full flex flex-col">
      ${nt()}
    </div>
  `,document.body.appendChild(f),requestAnimationFrame(()=>{f==null||f.classList.remove("translate-x-full")}),at();const e=t=>{t.key==="Escape"&&J&&se()};document.addEventListener("keydown",e),f._cleanup=()=>{document.removeEventListener("keydown",e)}}function mr(){if(f){const e=f._cleanup;e&&e(),f.remove(),f=null}J=!1,y.doc1=null,y.doc2=null,y.error=null,X=null,Y=null}const b={loading:!1,error:null,doc:null,sections:[]};let V=!1,x=null,Z=null;function H(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function je(e){const t=e.split(`
`);let r=0;for(const s of t){const n=s.trim();(n.startsWith("- ")||n.startsWith("* ")||/^\d+\.\s/.test(n))&&r++}return r}function pr(e){const t=e.split(`
`),r=[];let s="Introduction",n=0,a=1,i=[];for(let l=0;l<t.length;l++){const m=l+1,$=t[l];if($.startsWith("<!-- factbase:"))continue;const E=br($);if(E){if(i.length>0){const M=i.join(`
`).trim();M&&r.push({title:s,level:n,startLine:a,endLine:m-1,content:M,factCount:je(M)})}s=E.title,n=E.level,a=m,i=[]}else i.push($)}if(i.length>0){const l=i.join(`
`).trim();l&&r.push({title:s,level:n,startLine:a,endLine:t.length,content:l,factCount:je(l)})}return r}function br(e){const t=e.trimStart();if(!t.startsWith("#"))return null;let r=0;for(const n of t)if(n==="#")r++;else break;if(r===0||r>6)return null;const s=t.slice(r).trim();return s?{level:r,title:s}:null}function fr(e,t){const r=e.level>0?`<span class="text-xs text-gray-400 dark:text-gray-500">H${e.level}</span>`:'<span class="text-xs text-gray-400 dark:text-gray-500">Intro</span>',s=e.content.split(`
`).slice(0,5),n=e.content.split(`
`).length>5;return`
    <div class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <!-- Section Header -->
      <div class="p-3 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-2">
            <span class="text-xs font-medium text-gray-500 dark:text-gray-400">Section ${t+1}</span>
            ${r}
          </div>
          <div class="flex items-center space-x-2">
            <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300">
              ${e.factCount} fact${e.factCount!==1?"s":""}
            </span>
            <span class="text-xs text-gray-400 dark:text-gray-500">
              Lines ${e.startLine}-${e.endLine}
            </span>
          </div>
        </div>
        <h4 class="mt-1 text-sm font-semibold text-gray-900 dark:text-white">
          ${H(e.title)}
        </h4>
      </div>
      <!-- Section Preview -->
      <div class="p-3 font-mono text-xs text-gray-600 dark:text-gray-400 max-h-32 overflow-hidden">
        ${s.map(a=>`<div class="truncate">${H(a)||"&nbsp;"}</div>`).join("")}
        ${n?'<div class="text-gray-400 dark:text-gray-500 mt-1">...</div>':""}
      </div>
    </div>
  `}function it(){if(b.loading)return`
      <div class="flex items-center justify-center h-64">
        <div class="text-center">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading document...</p>
        </div>
      </div>
    `;if(b.error)return`
      <div class="p-4 text-center">
        <p class="text-red-600 dark:text-red-400">${H(b.error)}</p>
        <button id="split-preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;if(!b.doc)return`
      <div class="p-4 text-center text-gray-500 dark:text-gray-400">
        No document loaded
      </div>
    `;const e=b.sections.reduce((r,s)=>r+s.factCount,0),t=b.sections.filter(r=>r.content.length>=50);return`
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-semibold text-gray-900 dark:text-white">Split Preview</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400">Review sections before splitting</p>
          </div>
          <button id="split-preview-close-btn" class="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
            </svg>
          </button>
        </div>
      </div>

      <!-- Document Info -->
      <div class="flex-shrink-0 p-4 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-center justify-between">
          <div>
            <h4 class="text-sm font-semibold text-gray-900 dark:text-white">${H(b.doc.title)}</h4>
            <div class="mt-1 flex items-center space-x-2">
              <span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
                ${H(b.doc.doc_type)}
              </span>
              <span class="text-xs text-gray-400 dark:text-gray-500">[${H(b.doc.id)}]</span>
            </div>
          </div>
          <div class="text-right text-sm">
            <div class="text-gray-600 dark:text-gray-300">
              <span class="font-medium">${b.sections.length}</span> sections
            </div>
            <div class="text-gray-600 dark:text-gray-300">
              <span class="font-medium">${e}</span> total facts
            </div>
          </div>
        </div>
        ${t.length<2?`
          <div class="mt-3 p-2 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded text-xs text-amber-700 dark:text-amber-300">
            <strong>Note:</strong> This document has fewer than 2 sections with sufficient content (50+ chars). 
            Split may not be recommended.
          </div>
        `:""}
      </div>

      <!-- Sections List -->
      <div class="flex-1 overflow-auto p-4">
        <div class="space-y-3">
          ${b.sections.map((r,s)=>fr(r,s)).join("")}
        </div>
        ${b.sections.length===0?`
          <div class="text-center text-gray-500 dark:text-gray-400 py-8">
            No sections found in document
          </div>
        `:""}
      </div>

      <!-- Actions -->
      <div class="flex-shrink-0 p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <p class="text-xs text-gray-500 dark:text-gray-400">
            Split requires CLI: <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize split ${Z||"doc_id"}</code>
          </p>
          <div class="flex items-center space-x-2">
            <button
              id="split-preview-dismiss-btn"
              class="px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            >
              Close
            </button>
            <button
              id="split-preview-copy-btn"
              class="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            >
              Copy Command
            </button>
          </div>
        </div>
      </div>
    </div>
  `}function He(){if(!x)return;const e=x.querySelector("#split-preview-content");e&&(e.innerHTML=it(),ot())}function ot(){var e,t,r,s;(e=document.getElementById("split-preview-close-btn"))==null||e.addEventListener("click",ne),(t=document.getElementById("split-preview-close-error"))==null||t.addEventListener("click",ne),(r=document.getElementById("split-preview-dismiss-btn"))==null||r.addEventListener("click",ne),(s=document.getElementById("split-preview-copy-btn"))==null||s.addEventListener("click",()=>{const n=`factbase organize split ${Z}`;navigator.clipboard.writeText(n).then(()=>{const a=document.getElementById("split-preview-copy-btn");a&&(a.textContent="Copied!",setTimeout(()=>{a.textContent="Copy Command"},2e3))}).catch(()=>{alert(`Run: ${n}`)})})}async function xr(e){b.loading=!0,b.error=null,b.doc=null,b.sections=[],He();try{const t=await k.getDocument(e);b.doc=t,t.content&&(b.sections=pr(t.content))}catch(t){t instanceof C?b.error=t.message:b.error="Failed to load document"}finally{b.loading=!1,He()}}function yr(e){Z=e,V||(vr(),V=!0),xr(e)}function ne(){x&&(x.classList.add("translate-x-full"),setTimeout(()=>{x==null||x.remove(),x=null},300)),V=!1,b.doc=null,b.sections=[],b.error=null,Z=null}function vr(){x==null||x.remove(),x=document.createElement("div"),x.id="split-preview-panel",x.className=`
    fixed top-0 right-0 h-full w-full sm:w-[480px] lg:w-[560px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g," "),x.innerHTML=`
    <div id="split-preview-content" class="h-full flex flex-col">
      ${it()}
    </div>
  `,document.body.appendChild(x),requestAnimationFrame(()=>{x==null||x.classList.remove("translate-x-full")}),ot();const e=t=>{t.key==="Escape"&&V&&ne()};document.addEventListener("keydown",e),x._cleanup=()=>{document.removeEventListener("keydown",e)}}function hr(){if(x){const e=x._cleanup;e&&e(),x.remove(),x=null}V=!1,b.doc=null,b.sections=[],b.error=null,Z=null}const c={data:null,repos:[],loading:!0,error:null,filterRepo:"",filterType:"",successMessage:null},kr=["merge","misplaced","duplicate"];function D(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}async function N(){c.loading=!0,c.error=null,ve();try{const e={};c.filterRepo&&(e.repo=c.filterRepo),c.filterType&&(e.type=c.filterType);const[t,r]=await Promise.all([k.getSuggestions(e),c.repos.length===0?k.getRepositories():Promise.resolve({repositories:c.repos})]);c.data=t,c.repos=r.repositories}catch(e){e instanceof C?c.error=e.message:c.error="Failed to load suggestions"}finally{c.loading=!1,ve()}}function wr(){const e=c.repos.map(r=>`<option value="${D(r.id)}" ${c.filterRepo===r.id?"selected":""}>${D(r.name)}</option>`).join(""),t=kr.map(r=>`<option value="${r}" ${c.filterType===r?"selected":""}>${r}</option>`).join("");return`
    <div class="flex flex-col sm:flex-row gap-4">
      <div class="flex-1 sm:flex-none">
        <label for="filter-repo" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Repository</label>
        <select id="filter-repo" class="block w-full sm:w-48 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
          <option value="">All repositories</option>
          ${e}
        </select>
      </div>
      <div class="flex-1 sm:flex-none">
        <label for="filter-type" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Suggestion Type</label>
        <select id="filter-type" class="block w-full sm:w-48 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
          <option value="">All types</option>
          ${t}
        </select>
      </div>
    </div>
  `}function dt(){if(!c.data)return"";const e=c.data.merge.length,t=c.data.misplaced.length,r=c.data.duplicate_entries.length,s=[];return e>0&&s.push(`${I("merge")} <span class="ml-1 text-gray-600 dark:text-gray-400">${e}</span>`),t>0&&s.push(`${I("misplaced")} <span class="ml-1 text-gray-600 dark:text-gray-400">${t}</span>`),r>0&&s.push(`${I("duplicate")} <span class="ml-1 text-gray-600 dark:text-gray-400">${r}</span>`),`
    <div class="flex items-center justify-between text-sm">
      <div class="flex items-center space-x-4">
        ${s.length>0?s.join('<span class="mx-2 text-gray-300 dark:text-gray-600">|</span>'):'<span class="text-gray-500 dark:text-gray-400">No pending suggestions</span>'}
      </div>
      <div class="text-gray-500 dark:text-gray-400">
        ${c.data.total} total suggestion${c.data.total!==1?"s":""}
      </div>
    </div>
  `}function $r(){return!c.data||c.data.merge.length===0||c.filterType&&c.filterType!=="merge"?"":`
    <div class="space-y-4">
      <h3 class="text-lg font-medium text-gray-900 dark:text-white flex items-center space-x-2">
        ${I("merge")}
        <span>Merge Candidates</span>
        <span class="text-sm font-normal text-gray-500 dark:text-gray-400">(${c.data.merge.length})</span>
      </h3>
      <div class="space-y-3">
        ${c.data.merge.map((e,t)=>dr(e,t)).join("")}
      </div>
    </div>
  `}function Er(){return!c.data||c.data.misplaced.length===0||c.filterType&&c.filterType!=="misplaced"?"":`
    <div class="space-y-4">
      <h3 class="text-lg font-medium text-gray-900 dark:text-white flex items-center space-x-2">
        ${I("misplaced")}
        <span>Misplaced Documents</span>
        <span class="text-sm font-normal text-gray-500 dark:text-gray-400">(${c.data.misplaced.length})</span>
      </h3>
      <div class="space-y-3">
        ${c.data.misplaced.map((e,t)=>lr(e,t)).join("")}
      </div>
    </div>
  `}function Lr(){if(!c.data||c.data.duplicate_entries.length===0||c.filterType&&c.filterType!=="duplicate")return"";const e=c.data.duplicate_entries.map(t=>{const r=t.entries.map(s=>`<li class="text-sm text-gray-600 dark:text-gray-400">
        <span class="font-medium text-gray-900 dark:text-white">${D(s.doc_title)}</span>
        <span class="text-gray-400 dark:text-gray-500">(${D(s.doc_id)})</span>
        §${D(s.section)} line ${s.line_start}, ${s.facts.length} fact${s.facts.length!==1?"s":""}
      </li>`).join("");return`
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 border border-gray-200 dark:border-gray-700">
        <div class="flex items-center space-x-2 mb-2">
          ${I("duplicate")}
          <span class="font-medium text-gray-900 dark:text-white">${D(t.entity_name)}</span>
          <span class="text-sm text-gray-500 dark:text-gray-400">in ${t.entries.length} documents</span>
        </div>
        <ul class="list-disc list-inside space-y-1">${r}</ul>
      </div>
    `}).join("");return`
    <div class="space-y-4">
      <h3 class="text-lg font-medium text-gray-900 dark:text-white flex items-center space-x-2">
        ${I("duplicate")}
        <span>Duplicate Entries</span>
        <span class="text-sm font-normal text-gray-500 dark:text-gray-400">(${c.data.duplicate_entries.length})</span>
      </h3>
      <div class="space-y-3">${e}</div>
    </div>
  `}function ve(){const e=document.getElementById("organize-content");if(!e)return;const t=document.getElementById("organize-summary");t&&c.data&&(t.innerHTML=dt());const r=document.getElementById("organize-message");if(r&&(r.innerHTML=""),c.loading){e.innerHTML=Fe(4);return}if(c.error){e.innerHTML=de({title:"Error loading suggestions",message:c.error,onRetry:N}),le(N);return}if(!c.data||c.data.total===0){e.innerHTML=`
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No pending organize suggestions</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Run <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize analyze</code> to detect suggestions</p>
      </div>
    `;return}const s=$r(),n=Er(),a=Lr();if(!s&&!n&&!a){e.innerHTML=`
      <div class="text-center py-8">
        <span class="text-4xl">🔍</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No suggestions match current filters</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Try adjusting the filters above</p>
      </div>
    `;return}e.innerHTML=`
    <div class="space-y-8">
      ${s}
      ${n}
      ${a}
    </div>
  `,Cr(e),Mr(e)}function Sr(){const e=document.getElementById("filter-repo"),t=document.getElementById("filter-type");e==null||e.addEventListener("change",r=>{c.filterRepo=r.target.value,N()}),t==null||t.addEventListener("change",r=>{c.filterType=r.target.value,N()})}function Cr(e){e.querySelectorAll(".compare-btn").forEach(t=>{t.addEventListener("click",r=>{const s=r.currentTarget,n=s.dataset.doc1,a=s.dataset.doc2;n&&a&&ur(n,a)})}),e.querySelectorAll(".sections-btn").forEach(t=>{t.addEventListener("click",r=>{const n=r.currentTarget.dataset.docId;n&&yr(n)})}),e.querySelectorAll(".approve-btn").forEach(t=>{t.addEventListener("click",async r=>{const s=r.currentTarget,a=s.dataset.type==="merge"?`To merge these documents, run: factbase organize merge ${s.dataset.doc1} ${s.dataset.doc2}`:`To retype this document, run: factbase organize retype ${s.dataset.docId} --type ${s.dataset.suggestedType}`;alert(a)})}),e.querySelectorAll(".dismiss-btn").forEach(t=>{t.addEventListener("click",async r=>{const s=r.currentTarget,n=s.dataset.type,a=s.dataset.docId,i=s.dataset.targetId;s.textContent="Dismissing...",s.disabled=!0;try{await k.dismissSuggestion(n,a,i),w.success("Suggestion dismissed"),N()}catch(l){l instanceof C?w.error(`Failed to dismiss: ${l.message}`):w.error("Failed to dismiss suggestion"),ve()}})})}function Mr(e){e.querySelectorAll(".preview-doc-btn").forEach(t=>{t.addEventListener("click",r=>{const s=r.currentTarget.dataset.docId;s&&G(s)})})}function Tr(){return`
    <div class="space-y-4 sm:space-y-6">
      <div class="flex items-center justify-between">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Organize Suggestions</h2>
      </div>
      <div id="organize-message"></div>
      <div id="organize-filters" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${wr()}
      </div>
      <div id="organize-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${dt()}
      </div>
      <div id="organize-content">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading suggestions...</p>
        </div>
      </div>
    </div>
  `}function Ir(){Sr(),N()}function Ar(){c.data=null,c.loading=!0,c.error=null,c.successMessage=null,$e(),mr(),hr()}function A(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function _r(e,t,r={}){const{showCheckbox:s=!1}=r,n=`orphan-${t}-${e.line_number}`,a=e.source_doc?`<span class="text-xs text-gray-500 dark:text-gray-400">from <button class="preview-source-btn text-blue-600 dark:text-blue-400 hover:underline" data-doc-id="${A(e.source_doc)}" data-line="${e.source_line||""}">${A(e.source_doc)}</button>${e.source_line?` line ${e.source_line}`:""}</span>`:"",i=e.answered?`<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300">
        Assigned: ${A(e.answer||"")}
      </span>`:`<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-amber-100 dark:bg-amber-900/30 text-amber-800 dark:text-amber-300">
        Pending
      </span>`,l=s?`<input type="checkbox" id="${n}" class="orphan-checkbox h-4 w-4 text-blue-600 rounded border-gray-300 dark:border-gray-600 dark:bg-gray-700 focus:ring-blue-500" data-repo="${A(t)}" data-line="${e.line_number}">`:"",m=e.answered?"":`<div class="orphan-assign-form mt-3 pt-3 border-t border-gray-200 dark:border-gray-700">
        <div class="flex items-center space-x-2">
          <input
            type="text"
            class="orphan-target-input flex-1 text-sm rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500"
            placeholder="Document ID (6 chars) or 'dismiss'"
            data-repo="${A(t)}"
            data-line="${e.line_number}"
          >
          <button
            class="orphan-assign-btn px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
            data-repo="${A(t)}"
            data-line="${e.line_number}"
          >
            Assign
          </button>
          <button
            class="orphan-dismiss-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 rounded-md hover:bg-gray-200 dark:hover:bg-gray-600"
            data-repo="${A(t)}"
            data-line="${e.line_number}"
          >
            Dismiss
          </button>
        </div>
        <p class="orphan-error mt-1 text-sm text-red-600 dark:text-red-400 hidden"></p>
      </div>`;return`
    <div class="orphan-card p-4 bg-gray-50 dark:bg-gray-700/50 rounded-lg ${e.answered?"opacity-60":""}" data-line="${e.line_number}">
      <div class="flex items-start space-x-3">
        ${l?`<div class="pt-0.5">${l}</div>`:""}
        <div class="flex-1 min-w-0">
          <div class="flex items-center justify-between mb-2">
            ${i}
            ${a}
          </div>
          <p class="text-sm text-gray-900 dark:text-white whitespace-pre-wrap">${A(e.content)}</p>
          ${m}
        </div>
      </div>
    </div>
  `}const o={data:null,repos:[],loading:!0,error:null,selectedRepo:"",successMessage:null,bulkMode:!1,selectedLines:new Set};function De(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}async function qr(){try{const e=await k.getRepositories();o.repos=e.repositories,!o.selectedRepo&&o.repos.length>0&&(o.selectedRepo=o.repos[0].id)}catch(e){e instanceof C?o.error=e.message:o.error="Failed to load repositories"}}async function F(){if(!o.selectedRepo){o.data=null,o.loading=!1,O();return}o.loading=!0,o.error=null,O();try{const e=await k.getOrphans(o.selectedRepo);o.data=e}catch(e){e instanceof C?o.error=e.message:o.error="Failed to load orphans",o.data=null}finally{o.loading=!1,O()}}async function ze(e,t){if(o.selectedRepo)try{await k.assignOrphan(o.selectedRepo,e,t),w.success(t==="dismiss"?"Orphan dismissed":`Orphan assigned to ${t}`),await F()}catch(r){throw r instanceof C?r:new Error("Failed to assign orphan")}}async function Pe(e){if(!o.selectedRepo||o.selectedLines.size===0)return;const t=Array.from(o.selectedLines);let r=0;const s=[];for(const n of t)try{await k.assignOrphan(o.selectedRepo,n,e),r++}catch(a){s.push(`Line ${n}: ${a instanceof Error?a.message:"Unknown error"}`)}r>0&&w.success(e==="dismiss"?`Dismissed ${r} orphan(s)`:`Assigned ${r} orphan(s) to ${e}`),s.length>0&&w.error(`Some assignments failed: ${s.join("; ")}`),o.selectedLines.clear(),o.bulkMode=!1,await F()}function Br(){const e=o.repos.map(t=>`<option value="${De(t.id)}" ${o.selectedRepo===t.id?"selected":""}>${De(t.name)}</option>`).join("");return`
    <div>
      <label for="repo-select" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Repository</label>
      <select id="repo-select" class="block w-full sm:w-64 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
        ${o.repos.length===0?'<option value="">No repositories</option>':""}
        ${e}
      </select>
    </div>
  `}function lt(){return o.data?`
    <div class="flex items-center justify-between text-sm">
      <div class="text-gray-600 dark:text-gray-400">
        ${o.data.unanswered} pending / ${o.data.total} total orphans
      </div>
      ${o.data.answered>0?`<div class="text-green-600 dark:text-green-400">${o.data.answered} assigned</div>`:""}
    </div>
  `:""}function Rr(){return!o.bulkMode||o.selectedLines.size===0?"":`
    <div class="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-4">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <span class="text-sm text-blue-700 dark:text-blue-300">
          ${o.selectedLines.size} orphan(s) selected
        </span>
        <div class="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
          <input
            type="text"
            id="bulk-target-input"
            class="text-sm rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500"
            placeholder="Document ID"
          >
          <div class="flex gap-2">
            <button
              id="bulk-assign-btn"
              class="flex-1 sm:flex-none px-3 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700"
            >
              Assign All
            </button>
            <button
              id="bulk-dismiss-btn"
              class="flex-1 sm:flex-none px-3 py-2 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 rounded-md hover:bg-gray-200 dark:hover:bg-gray-600"
            >
              Dismiss All
            </button>
          </div>
        </div>
      </div>
    </div>
  `}function O(){const e=document.getElementById("orphans-content");if(!e)return;const t=document.getElementById("orphans-summary");t&&o.data&&(t.innerHTML=lt());const r=document.getElementById("bulk-actions-container");r&&(r.innerHTML=Rr(),Hr());const s=document.getElementById("orphans-message");if(s&&(s.innerHTML=""),o.loading){e.innerHTML=Fe(3);return}if(o.error){e.innerHTML=de({title:"Error loading orphans",message:o.error,onRetry:F}),le(F);return}if(!o.selectedRepo){e.innerHTML=`
      <div class="text-center py-8">
        <p class="text-gray-600 dark:text-gray-300">Select a repository to view orphans</p>
      </div>
    `;return}if(!o.data||o.data.orphans.length===0){e.innerHTML=`
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No orphaned facts</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Orphans are created during merge/split operations</p>
      </div>
    `;return}const n=[...o.data.orphans].sort((a,i)=>a.answered===i.answered?a.line_number-i.line_number:a.answered?1:-1);e.innerHTML=`
    <div class="space-y-3">
      ${n.map(a=>_r(a,o.selectedRepo,{showCheckbox:o.bulkMode})).join("")}
    </div>
  `,jr(e)}function jr(e){e.querySelectorAll(".orphan-assign-btn").forEach(t=>{t.addEventListener("click",async r=>{const s=r.currentTarget,n=parseInt(s.dataset.line||"0",10),a=s.closest(".orphan-card"),i=a==null?void 0:a.querySelector(".orphan-target-input"),l=a==null?void 0:a.querySelector(".orphan-error");if(!(i!=null&&i.value.trim())){l&&(l.textContent='Please enter a document ID or "dismiss"',l.classList.remove("hidden"));return}s.disabled=!0,s.textContent="Assigning...";try{await ze(n,i.value.trim())}catch(m){l&&(l.textContent=m instanceof Error?m.message:"Failed to assign",l.classList.remove("hidden")),s.disabled=!1,s.textContent="Assign"}})}),e.querySelectorAll(".orphan-dismiss-btn").forEach(t=>{t.addEventListener("click",async r=>{const s=r.currentTarget,n=parseInt(s.dataset.line||"0",10),a=s.closest(".orphan-card"),i=a==null?void 0:a.querySelector(".orphan-error");s.disabled=!0,s.textContent="Dismissing...";try{await ze(n,"dismiss")}catch(l){i&&(i.textContent=l instanceof Error?l.message:"Failed to dismiss",i.classList.remove("hidden")),s.disabled=!1,s.textContent="Dismiss"}})}),e.querySelectorAll(".orphan-checkbox").forEach(t=>{t.addEventListener("change",r=>{const s=r.target,n=parseInt(s.dataset.line||"0",10);s.checked?o.selectedLines.add(n):o.selectedLines.delete(n),O()})}),e.querySelectorAll(".preview-source-btn").forEach(t=>{t.addEventListener("click",r=>{r.preventDefault();const s=r.currentTarget.dataset.docId,n=r.currentTarget.dataset.line;s&&G(s,n?parseInt(n,10):void 0)})})}function Hr(){const e=document.getElementById("bulk-assign-btn"),t=document.getElementById("bulk-dismiss-btn"),r=document.getElementById("bulk-target-input");e==null||e.addEventListener("click",async()=>{const s=r==null?void 0:r.value.trim();if(!s){alert("Please enter a document ID");return}confirm(`Assign ${o.selectedLines.size} orphan(s) to ${s}?`)&&await Pe(s)}),t==null||t.addEventListener("click",async()=>{confirm(`Dismiss ${o.selectedLines.size} orphan(s)?`)&&await Pe("dismiss")})}function Dr(){const e=document.getElementById("repo-select");e==null||e.addEventListener("change",t=>{o.selectedRepo=t.target.value,o.selectedLines.clear(),F()})}function zr(){o.bulkMode=!o.bulkMode,o.bulkMode||o.selectedLines.clear(),O()}function Pr(){const e=o.bulkMode?"Exit bulk mode":"Bulk actions";return`
    <div class="space-y-4 sm:space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Orphaned Facts</h2>
        <button
          id="bulk-mode-toggle"
          class="inline-flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md ${o.bulkMode?"bg-blue-600 text-white hover:bg-blue-700":"bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600"}"
        >
          ${e}
        </button>
      </div>
      <div id="orphans-message"></div>
      <div id="bulk-actions-container"></div>
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${Br()}
      </div>
      <div id="orphans-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${lt()}
      </div>
      <div id="orphans-content" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading...</p>
        </div>
      </div>
    </div>
  `}async function Or(){var e;await qr(),Dr(),(e=document.getElementById("bulk-mode-toggle"))==null||e.addEventListener("click",zr),o.selectedRepo?await F():(o.loading=!1,O())}function Nr(){o.data=null,o.loading=!0,o.error=null,o.successMessage=null,o.bulkMode=!1,o.selectedLines.clear(),$e()}const Fr=".document-group, .suggestion-card, .orphan-card, .question-card";let S=-1,pe=null,ie=!1;function ee(){return Array.from(document.querySelectorAll(Fr))}function oe(e){const t=ee();if(t.length===0)return;const r=Math.max(0,Math.min(e,t.length-1));S>=0&&S<t.length&&t[S].classList.remove("keyboard-focused"),S=r;const s=t[S];s.classList.add("keyboard-focused"),s.scrollIntoView({behavior:"smooth",block:"nearest"}),s.setAttribute("tabindex","0"),s.focus()}function Ur(){ee().length!==0&&(S<0?oe(0):oe(S+1))}function Qr(){const e=ee();e.length!==0&&(S<0?oe(e.length-1):oe(S-1))}function Wr(){const e=ee();if(S<0||S>=e.length)return;const t=e[S],r=t.querySelector(".preview-doc-btn, .preview-line-btn, .compare-btn, .sections-btn");if(r){r.click();return}const s=t.querySelector('textarea[name="answer"]');if(s){s.focus();return}const n=t.querySelector("button");n&&n.click()}function Kr(){if(ie){he();return}const e=document.getElementById("document-preview-panel"),t=document.getElementById("merge-preview-panel"),r=document.getElementById("split-preview-panel");if(e&&!e.classList.contains("hidden")){const s=e.querySelector(".close-preview-btn");s==null||s.click();return}if(t&&!t.classList.contains("hidden")){const s=t.querySelector(".close-preview-btn");s==null||s.click();return}if(r&&!r.classList.contains("hidden")){const s=r.querySelector(".close-preview-btn");s==null||s.click();return}Gr()}function Gr(){ee().forEach(t=>{t.classList.remove("keyboard-focused"),t.removeAttribute("tabindex")}),S=-1}function Jr(){var t,r;if(ie)return;ie=!0;const e=document.createElement("div");e.id="keyboard-help-modal",e.className="fixed inset-0 z-50 flex items-center justify-center bg-black/50",e.setAttribute("role","dialog"),e.setAttribute("aria-modal","true"),e.setAttribute("aria-labelledby","keyboard-help-title"),e.innerHTML=`
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full mx-4 p-6" role="document">
      <div class="flex items-center justify-between mb-4">
        <h3 id="keyboard-help-title" class="text-lg font-semibold text-gray-900 dark:text-white">Keyboard Shortcuts</h3>
        <button id="close-help-modal" class="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300" aria-label="Close keyboard shortcuts">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
          </svg>
        </button>
      </div>
      <div class="space-y-3" role="list" aria-label="Keyboard shortcuts list">
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Next item</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="j key">j</kbd>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Previous item</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="k key">k</kbd>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Expand / Preview</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono">Enter</kbd>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Close / Cancel</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono">Esc</kbd>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Show this help</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="question mark key">?</kbd>
        </div>
        <hr class="border-gray-200 dark:border-gray-700" aria-hidden="true" />
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Submit answer</span>
          <div class="flex items-center space-x-1">
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono">Ctrl</kbd>
            <span class="text-gray-400" aria-hidden="true">+</span>
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono">Enter</kbd>
          </div>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Go to Dashboard</span>
          <div class="flex items-center space-x-1">
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="g key">g</kbd>
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="d key">d</kbd>
          </div>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Go to Review</span>
          <div class="flex items-center space-x-1">
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="g key">g</kbd>
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="r key">r</kbd>
          </div>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Go to Organize</span>
          <div class="flex items-center space-x-1">
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="g key">g</kbd>
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="o key">o</kbd>
          </div>
        </div>
      </div>
      <p class="mt-4 text-xs text-gray-500 dark:text-gray-400">Press <kbd class="px-1 bg-gray-100 dark:bg-gray-700 rounded text-xs">Esc</kbd> to close</p>
    </div>
  `,document.body.appendChild(e),e.addEventListener("click",s=>{s.target===e&&he()}),(t=document.getElementById("close-help-modal"))==null||t.addEventListener("click",he),(r=document.getElementById("close-help-modal"))==null||r.focus()}function he(){const e=document.getElementById("keyboard-help-modal");e&&e.remove(),ie=!1}let re=!1,U=null;function Vr(e){const t=e.target;if(t.tagName==="INPUT"||t.tagName==="TEXTAREA"||t.tagName==="SELECT"){e.key==="Escape"&&(t.blur(),e.preventDefault());return}if(re)switch(re=!1,U&&(clearTimeout(U),U=null),e.key.toLowerCase()){case"d":window.location.hash="#/",e.preventDefault();return;case"r":window.location.hash="#/review",e.preventDefault();return;case"o":window.location.hash="#/organize",e.preventDefault();return}switch(e.key.toLowerCase()){case"j":Ur(),e.preventDefault();break;case"k":Qr(),e.preventDefault();break;case"enter":S>=0&&(Wr(),e.preventDefault());break;case"escape":Kr(),e.preventDefault();break;case"?":Jr(),e.preventDefault();break;case"g":re=!0,U=window.setTimeout(()=>{re=!1,U=null},500),e.preventDefault();break}}function Xr(){pe||(pe=Vr,document.addEventListener("keydown",pe))}function Yr(){S=-1}let L=!1;function Zr(e){switch(e){case"/":return Ce();case"/review":return rr();case"/organize":return Tr();case"/orphans":return Pr();default:return Ce()}}function es(e){return ke.map(t=>{const r=t.path===e,s="flex items-center space-x-2 px-3 py-2 rounded-md text-sm font-medium transition-colors",n=r?"bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-200":"text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700";return`<a href="#${t.path}" class="${s} ${n}">
      <span>${t.icon}</span>
      <span>${t.title}</span>
    </a>`}).join("")}function ts(e){return ke.map(t=>{const r=t.path===e,s="flex items-center space-x-3 px-4 py-3 text-base font-medium transition-colors",n=r?"bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-200":"text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700";return`<a href="#${t.path}" class="mobile-nav-link ${s} ${n}">
      <span class="text-xl">${t.icon}</span>
      <span>${t.title}</span>
    </a>`}).join("")}function rs(){return`
    <button id="mobile-menu-btn" class="md:hidden p-2 rounded-md text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700" aria-label="${L?"Close menu":"Open menu"}" aria-expanded="${L}" aria-controls="mobile-nav">
      <svg class="w-6 h-6 ${L?"hidden":"block"}" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16"></path>
      </svg>
      <svg class="w-6 h-6 ${L?"block":"hidden"}" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
      </svg>
    </button>
  `}let Oe=null;function ss(e){e==="/"||e===null?Ct():e==="/review"?ar():e==="/organize"?Ar():e==="/orphans"&&Nr()}function ns(e){e==="/"?St():e==="/review"?sr():e==="/organize"?Ir():e==="/orphans"&&Or(),Yr()}function ct(e){var s;ss(Oe),Oe=e,L=!1;const t=((s=ke.find(n=>n.path===e))==null?void 0:s.title)||"Dashboard",r=document.querySelector("#app");r.innerHTML=`
    <a href="#main-content" class="skip-link">Skip to main content</a>
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900">
      <header class="bg-white dark:bg-gray-800 shadow" role="banner">
        <div class="max-w-7xl mx-auto px-4 py-4">
          <div class="flex items-center justify-between">
            <a href="#/" class="text-xl font-bold text-blue-600 dark:text-blue-400" aria-label="Factbase - Go to dashboard">Factbase</a>
            <!-- Desktop navigation -->
            <nav class="hidden md:flex space-x-1" role="navigation" aria-label="Main navigation">
              ${es(e)}
            </nav>
            <!-- Mobile menu button -->
            ${rs()}
          </div>
        </div>
        <!-- Mobile navigation -->
        <nav id="mobile-nav" class="md:hidden ${L?"block":"hidden"} border-t border-gray-200 dark:border-gray-700" role="navigation" aria-label="Mobile navigation">
          <div class="px-2 py-2 space-y-1">
            ${ts(e)}
          </div>
        </nav>
      </header>
      <main id="main-content" class="max-w-7xl mx-auto px-4 py-6 sm:py-8" role="main" aria-label="${t}">
        ${Zr(e)}
      </main>
    </div>
  `,as(),ns(e)}function as(){const e=document.getElementById("mobile-menu-btn");e==null||e.addEventListener("click",()=>{var r,s,n,a;L=!L;const t=document.getElementById("mobile-nav");if(t&&t.classList.toggle("hidden",!L),e){e.setAttribute("aria-expanded",String(L));const i=e.querySelectorAll("svg");(r=i[0])==null||r.classList.toggle("hidden",L),(s=i[0])==null||s.classList.toggle("block",!L),(n=i[1])==null||n.classList.toggle("hidden",!L),(a=i[1])==null||a.classList.toggle("block",L)}}),document.querySelectorAll(".mobile-nav-link").forEach(t=>{t.addEventListener("click",()=>{L=!1})})}Ne.onRouteChange(e=>{ct(e)});Xr();Ue();ct(Ne.getCurrentRoute());
