var it=Object.defineProperty;var ot=(e,t,r)=>t in e?it(e,t,{enumerable:!0,configurable:!0,writable:!0,value:r}):e[t]=r;var Le=(e,t,r)=>ot(e,typeof t!="symbol"?t+"":t,r);(function(){const t=document.createElement("link").relList;if(t&&t.supports&&t.supports("modulepreload"))return;for(const n of document.querySelectorAll('link[rel="modulepreload"]'))s(n);new MutationObserver(n=>{for(const a of n)if(a.type==="childList")for(const i of a.addedNodes)i.tagName==="LINK"&&i.rel==="modulepreload"&&s(i)}).observe(document,{childList:!0,subtree:!0});function r(n){const a={};return n.integrity&&(a.integrity=n.integrity),n.referrerPolicy&&(a.referrerPolicy=n.referrerPolicy),n.crossOrigin==="use-credentials"?a.credentials="include":n.crossOrigin==="anonymous"?a.credentials="omit":a.credentials="same-origin",a}function s(n){if(n.ep)return;n.ep=!0;const a=r(n);fetch(n.href,a)}})();const he=[{path:"/",title:"Dashboard",icon:"📊"},{path:"/review",title:"Review Queue",icon:"❓"},{path:"/organize",title:"Organize",icon:"📁"},{path:"/orphans",title:"Orphans",icon:"📝"}];class dt{constructor(){Le(this,"handlers",[]);window.addEventListener("hashchange",()=>this.handleChange()),window.addEventListener("load",()=>this.handleChange())}handleChange(){const t=this.getCurrentRoute();this.handlers.forEach(r=>r(t))}getCurrentRoute(){const t=window.location.hash.slice(1)||"/";return["/","/review","/organize","/orphans"].includes(t)?t:"/"}navigate(t){window.location.hash=t}onRouteChange(t){this.handlers.push(t)}}const Pe=new dt,lt="";class ct{async request(t,r){const s=await fetch(`${lt}${t}`,{headers:{"Content-Type":"application/json"},...r});if(!s.ok){const n=await s.json().catch(()=>({error:`HTTP ${s.status}: ${s.statusText}`,code:"HTTP_ERROR"}));throw new S(n.error,n.code,s.status)}return s.json()}async getStats(){return this.request("/api/stats")}async getReviewStats(){return this.request("/api/stats/review")}async getOrganizeStats(){return this.request("/api/stats/organize")}async getReviewQueue(t){const r=new URLSearchParams;t!=null&&t.repo&&r.set("repo",t.repo),t!=null&&t.type&&r.set("type",t.type);const s=r.toString();return this.request(`/api/review/queue${s?`?${s}`:""}`)}async getDocumentReview(t){return this.request(`/api/review/queue/${encodeURIComponent(t)}`)}async answerQuestion(t,r,s){return this.request(`/api/review/answer/${encodeURIComponent(t)}`,{method:"POST",body:JSON.stringify({question_index:r,answer:s})})}async bulkAnswerQuestions(t){return this.request("/api/review/bulk-answer",{method:"POST",body:JSON.stringify({answers:t})})}async getReviewStatus(){return this.request("/api/review/status")}async getSuggestions(t){const r=new URLSearchParams;t!=null&&t.repo&&r.set("repo",t.repo),t!=null&&t.type&&r.set("type",t.type),(t==null?void 0:t.threshold)!==void 0&&r.set("threshold",t.threshold.toString());const s=r.toString();return this.request(`/api/organize/suggestions${s?`?${s}`:""}`)}async getDocumentSuggestions(t){return this.request(`/api/organize/suggestions/${encodeURIComponent(t)}`)}async dismissSuggestion(t,r,s){return this.request("/api/organize/dismiss",{method:"POST",body:JSON.stringify({type:t,doc_id:r,target_id:s})})}async getOrphans(t){return this.request(`/api/organize/orphans?repo=${encodeURIComponent(t)}`)}async assignOrphan(t,r,s){return this.request("/api/organize/assign-orphan",{method:"POST",body:JSON.stringify({repo:t,line_number:r,target:s})})}async getDocument(t,r){const s=new URLSearchParams;(r==null?void 0:r.include_preview)!==void 0&&s.set("include_preview",r.include_preview.toString()),(r==null?void 0:r.max_content_length)!==void 0&&s.set("max_content_length",r.max_content_length.toString());const n=s.toString();return this.request(`/api/documents/${encodeURIComponent(t)}${n?`?${n}`:""}`)}async getDocumentLinks(t){return this.request(`/api/documents/${encodeURIComponent(t)}/links`)}async getRepositories(){return this.request("/api/repos")}}class S extends Error{constructor(t,r,s){super(t),this.code=r,this.status=s,this.name="ApiRequestError"}get isNotFound(){return this.status===404||this.code==="NOT_FOUND"}get isBadRequest(){return this.status===400||this.code==="BAD_REQUEST"}get isServerError(){return this.status>=500}}const k=new ct;function ut(){return`
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
  `}function Oe(e=3){return`
    <div class="space-y-4" role="status" aria-label="Loading content">
      <span class="sr-only">Loading content...</span>
      ${Array(e).fill(0).map(()=>ut()).join("")}
    </div>
  `}function le(){return`
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
  `}function gt(){return`
    <dl class="grid grid-cols-2 md:grid-cols-4 gap-4 animate-pulse" aria-hidden="true">
      ${Array(4).fill(0).map(()=>`
        <div>
          <div class="h-3 bg-gray-200 dark:bg-gray-700 rounded w-20 mb-2"></div>
          <div class="h-6 bg-gray-300 dark:bg-gray-600 rounded w-12"></div>
        </div>
      `).join("")}
    </dl>
  `}function oe(e){const{title:t="Error",message:r,onRetry:s,retryLabel:n="Retry"}=e,a=s?`retry-${Date.now()}`:null;return`
    <div class="text-center py-8">
      <div class="inline-flex items-center justify-center w-12 h-12 rounded-full bg-red-100 dark:bg-red-900/30 mb-4">
        <svg class="w-6 h-6 text-red-600 dark:text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"></path>
        </svg>
      </div>
      <p class="font-medium text-red-600 dark:text-red-400">${ce(t)}</p>
      <p class="text-sm text-gray-600 dark:text-gray-400 mt-1">${ce(r)}</p>
      ${a?`
        <button id="${a}" class="mt-4 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 text-sm transition-colors">
          ${ce(n)}
        </button>
      `:""}
    </div>
  `}function de(e){const t=document.querySelectorAll('[id^="retry-"]'),r=t[t.length-1];r==null||r.addEventListener("click",e)}function ce(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}const u={stats:null,review:null,organize:null,loading:!0,error:null,autoRefresh:!1,refreshInterval:null},mt=3e4;function pt(e){return e<1024?`${e} B`:e<1024*1024?`${(e/1024).toFixed(1)} KB`:`${(e/(1024*1024)).toFixed(1)} MB`}function bt(e){return e?new Date(e).toLocaleString():"Never"}async function ne(){u.loading=!0,u.error=null,Ee();try{const[e,t,r]=await Promise.all([k.getStats(),k.getReviewStats(),k.getOrganizeStats()]);u.stats=e,u.review=t,u.organize=r}catch(e){e instanceof S?u.error=e.message:u.error="Failed to load dashboard data"}finally{u.loading=!1,Ee()}}function Ee(){var a,i;const e=document.getElementById("review-count");e&&(e.textContent=u.loading?"...":((a=u.review)==null?void 0:a.unanswered.toString())??"-");const t=document.getElementById("organize-count");if(t){const l=u.organize?u.organize.merge_candidates+u.organize.misplaced_candidates+u.organize.duplicate_entry_count:0;t.textContent=u.loading?"...":l.toString()}const r=document.getElementById("orphan-count");r&&(r.textContent=u.loading?"...":((i=u.organize)==null?void 0:i.orphan_count.toString())??"-");const s=document.getElementById("stats-content");s&&(u.loading?s.innerHTML=gt():u.error?(s.innerHTML=oe({title:"Error loading stats",message:u.error,onRetry:ne}),de(ne)):u.stats&&(s.innerHTML=`
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
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${pt(u.stats.db_size_bytes)}</dd>
          </div>
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Last Scan</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${bt(u.stats.last_scan)}</dd>
          </div>
        </dl>
      `));const n=document.getElementById("auto-refresh-toggle");n&&(n.checked=u.autoRefresh)}function xt(e){u.autoRefresh=e,e&&!u.refreshInterval?u.refreshInterval=window.setInterval(()=>ne(),mt):!e&&u.refreshInterval&&(clearInterval(u.refreshInterval),u.refreshInterval=null)}function Se(){return`
    <div class="space-y-4 sm:space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Dashboard</h2>
        <label class="flex items-center space-x-2 text-sm text-gray-600 dark:text-gray-300">
          <input type="checkbox" id="auto-refresh-toggle" class="rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500" ${u.autoRefresh?"checked":""}>
          <span>Auto-refresh</span>
        </label>
      </div>
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
  `}function ft(){const e=document.getElementById("auto-refresh-toggle");e==null||e.addEventListener("change",t=>{xt(t.target.checked)}),ne()}function yt(){u.refreshInterval&&(clearInterval(u.refreshInterval),u.refreshInterval=null)}const U=new Map;function Fe(e,t){return`${e}:${t}`}function Ne(e,t){const r=Fe(e,t);return U.has(r)||U.set(r,{submitting:!1,error:null}),U.get(r)}function I(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function vt(e,t,r){const s=Ne(e,t),n=`answer-form-${I(e)}-${t}`,a=`answer-input-${I(e)}-${t}`,i=`answer-label-${I(e)}-${t}`,l=`answer-hint-${I(e)}-${t}`,h=ht(r),L=s.submitting?"opacity-50 pointer-events-none":"";return`
    <form id="${n}" class="answer-form mt-3 ${L}" data-doc-id="${I(e)}" data-question-index="${t}">
      <div class="space-y-2">
        <label id="${i}" for="${a}" class="sr-only">Answer for ${I(r)} question</label>
        <textarea
          id="${a}"
          name="answer"
          rows="2"
          class="block w-full rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm resize-none"
          placeholder="${I(h)}"
          aria-labelledby="${i}"
          aria-describedby="${l}"
          ${s.submitting?'disabled aria-busy="true"':""}
        ></textarea>
        <div id="${l}" class="sr-only">Press Ctrl+Enter to submit. Use Dismiss to skip or Delete fact to remove.</div>
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
          <div class="text-sm text-red-600 dark:text-red-400" role="alert">${I(s.error)}</div>
        `:""}
      </div>
    </form>
  `}function ht(e){switch(e){case"temporal":return'e.g., "Started March 2022, left December 2024"';case"conflict":return'e.g., "Both were part-time, no conflict" or explain resolution';case"missing":return'e.g., "LinkedIn profile, checked 2024-01-15"';case"ambiguous":return'e.g., "Home address" or "split: home in Austin, work in SF"';case"stale":return'e.g., "Still accurate as of today" or provide update';case"duplicate":return'e.g., "Keep this one" or "Merge into [other_id]"';default:return"Enter your answer..."}}async function ue(e,t,r,s){const n=Ne(e,t);n.submitting=!0,n.error=null;try{await k.answerQuestion(e,t,r),s.onSuccess(e,t,r),U.delete(Fe(e,t))}catch(a){a instanceof S?n.error=a.message:n.error="Failed to submit answer",s.onError(n.error)}finally{n.submitting=!1}}function kt(e,t){e.addEventListener("submit",async r=>{const s=r.target.closest(".answer-form");if(!s)return;r.preventDefault();const n=s.dataset.docId,a=parseInt(s.dataset.questionIndex||"0",10),i=s.querySelector("textarea"),l=(i==null?void 0:i.value.trim())||"";!n||!l||await ue(n,a,l,t)}),e.addEventListener("click",async r=>{const s=r.target.closest(".quick-action");if(!s)return;const n=s.closest(".answer-form");if(!n)return;const a=n.dataset.docId,i=parseInt(n.dataset.questionIndex||"0",10),l=s.dataset.action;if(!a||!l)return;await ue(a,i,l==="dismiss"?"dismiss":"delete",t)}),e.addEventListener("keydown",async r=>{if(r.key==="Enter"&&(r.ctrlKey||r.metaKey)){const s=r.target;if(s.tagName!=="TEXTAREA")return;const n=s.closest(".answer-form");if(!n)return;r.preventDefault();const a=n.dataset.docId,i=parseInt(n.dataset.questionIndex||"0",10),l=s.value.trim();if(!a||!l)return;await ue(a,i,l,t)}})}function wt(){U.clear()}const g={selections:new Set,submitting:!1,showBulkAnswer:!1};function ke(e,t){return`${e}:${t}`}function Ue(e){const[t,r]=e.split(":");return{docId:t,questionIndex:parseInt(r,10)}}function ge(){return Array.from(g.selections).map(Ue)}function $t(e,t){const r=ke(e,t);g.selections.has(r)?g.selections.delete(r):g.selections.add(r)}function Lt(e){g.selections.clear();for(const t of e)for(let r=0;r<t.questions.length;r++)t.questions[r].answered||g.selections.add(ke(t.doc_id,r))}function Et(){g.selections.clear()}function Qe(){g.selections.clear(),g.submitting=!1,g.showBulkAnswer=!1}function Ce(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function St(e){const t=g.selections.size,r=t>0;return`
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
      ${g.showBulkAnswer?Ct():""}
    </div>
  `}function Ct(){const e="bulk-answer-input",t="bulk-answer-label";return`
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
  `}function Mt(e,t,r){if(r.answered)return"";const s=ke(e,t),n=g.selections.has(s);return`
    <input
      type="checkbox"
      id="${`bulk-checkbox-${Ce(e)}-${t}`}"
      class="bulk-checkbox h-4 w-4 rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500"
      data-doc-id="${Ce(e)}"
      data-question-index="${t}"
      aria-label="Select question ${t+1} for bulk action"
      ${n?"checked":""}
    />
  `}async function Me(e,t,r){if(g.selections.size!==0){g.submitting=!0;try{const s=Array.from(g.selections).map(i=>{const{docId:l,questionIndex:h}=Ue(i);return{doc_id:l,question_index:h,answer:t}}),n=await k.bulkAnswerQuestions(s);n.errors&&n.errors.length>0&&r.onError(`Some answers failed: ${n.errors.join(", ")}`);const a=n.results.filter(i=>i.success).length;a>0&&(r.onSuccess(a,e==="dismiss"?"dismissed":"answered"),g.selections.clear(),g.showBulkAnswer=!1)}catch(s){s instanceof S?r.onError(s.message):r.onError("Failed to process bulk action")}finally{g.submitting=!1}}}function Tt(e,t,r,s){var n,a,i,l,h,L;(n=e.querySelector("#bulk-select-all"))==null||n.addEventListener("click",()=>{Lt(t),r.onSelectionChange(ge()),s()}),(a=e.querySelector("#bulk-select-none"))==null||a.addEventListener("click",()=>{Et(),r.onSelectionChange(ge()),s()}),(i=e.querySelector("#bulk-dismiss-btn"))==null||i.addEventListener("click",async()=>{if(g.selections.size===0)return;const E=g.selections.size;confirm(`Dismiss ${E} selected question(s)?`)&&(await Me("dismiss","dismiss",r),s())}),(l=e.querySelector("#bulk-answer-btn"))==null||l.addEventListener("click",()=>{g.submitting||(g.showBulkAnswer=!g.showBulkAnswer,s())}),(h=e.querySelector("#bulk-answer-cancel"))==null||h.addEventListener("click",()=>{g.showBulkAnswer=!1,s()}),(L=e.querySelector("#bulk-answer-submit"))==null||L.addEventListener("click",async()=>{const E=e.querySelector("#bulk-answer-input"),C=E==null?void 0:E.value.trim();C&&(await Me("answer",C,r),s())}),e.addEventListener("change",E=>{const C=E.target;if(!C.classList.contains("bulk-checkbox"))return;const $e=C.dataset.docId,at=parseInt(C.dataset.questionIndex||"0",10);$e&&($t($e,at),r.onSelectionChange(ge()),s())})}const It={temporal:{bg:"bg-blue-100 dark:bg-blue-900",text:"text-blue-700 dark:text-blue-200"},conflict:{bg:"bg-red-100 dark:bg-red-900",text:"text-red-700 dark:text-red-200"},missing:{bg:"bg-amber-100 dark:bg-amber-900",text:"text-amber-700 dark:text-amber-200"},ambiguous:{bg:"bg-purple-100 dark:bg-purple-900",text:"text-purple-700 dark:text-purple-200"},stale:{bg:"bg-gray-100 dark:bg-gray-700",text:"text-gray-700 dark:text-gray-200"},duplicate:{bg:"bg-green-100 dark:bg-green-900",text:"text-green-700 dark:text-green-200"}};function N(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function Ke(e){const t=It[e]||{bg:"bg-gray-100 dark:bg-gray-700",text:"text-gray-700 dark:text-gray-200"};return`<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${t.bg} ${t.text}">@q[${N(e)}]</span>`}function At(e,t,r,s={}){const{showAnswerForm:n=!0,showCheckbox:a=!1}=s,i=e.answered?"opacity-60":"",l=e.answered?'<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-200 ml-2">Answered</span>':"",h=a?Mt(t,r,e):"",L=e.line_ref?`<button
        class="preview-line-btn text-xs text-blue-600 dark:text-blue-400 hover:underline"
        data-doc-id="${N(t)}"
        data-line-ref="${e.line_ref}"
      >Line ${e.line_ref}</button>`:"",E=e.answered&&e.answer?`<div class="mt-2 p-2 bg-gray-50 dark:bg-gray-800 rounded text-sm text-gray-600 dark:text-gray-400">
        <span class="font-medium">Answer:</span> ${N(e.answer)}
      </div>`:n&&!e.answered?vt(t,r,e.question_type):"";return`
    <div class="question-card border border-gray-200 dark:border-gray-700 rounded-lg p-4 ${i}" data-doc-id="${N(t)}" data-question-index="${r}">
      <div class="flex items-start justify-between">
        <div class="flex items-center space-x-2">
          ${h}
          ${Ke(e.question_type)}
          ${l}
          ${L}
        </div>
      </div>
      <p class="mt-2 text-gray-700 dark:text-gray-300">${N(e.description)}</p>
      ${E}
    </div>
  `}const v={loading:!1,error:null,document:null,highlightLine:null};let K=!1,m=null;function R(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function Te(e){return!e||e.length===0?'<p class="text-sm text-gray-500 dark:text-gray-400">None</p>':`
    <ul class="space-y-1">
      ${e.map(t=>`
        <li>
          <button
            class="preview-link-btn text-sm text-blue-600 dark:text-blue-400 hover:underline text-left"
            data-doc-id="${R(t.id)}"
          >
            ${R(t.title)}
          </button>
        </li>
      `).join("")}
    </ul>
  `}function _t(e,t){return e.split(`
`).map((s,n)=>{const a=n+1,i=t!==null&&a===t;return`
      <div class="flex ${i?"bg-yellow-100 dark:bg-yellow-900/50 border-l-4 border-yellow-400":""}" ${i?'id="highlighted-line"':""}>
        <span class="select-none w-10 flex-shrink-0 text-right pr-3 ${i?"text-yellow-600 dark:text-yellow-400 font-bold":"text-gray-400 dark:text-gray-600"} text-xs leading-6">${a}</span>
        <pre class="flex-1 text-sm leading-6 whitespace-pre-wrap break-words text-gray-800 dark:text-gray-200">${R(s)||" "}</pre>
      </div>
    `}).join("")}function Ge(){if(v.loading)return`
      <div class="flex items-center justify-center h-64" role="status" aria-live="polite">
        <div class="text-center">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600" aria-hidden="true"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading document...</p>
        </div>
      </div>
    `;if(v.error)return`
      <div class="p-4 text-center" role="alert">
        <p class="text-red-600 dark:text-red-400">${R(v.error)}</p>
        <button id="preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;if(!v.document)return'<div class="p-4 text-gray-500 dark:text-gray-400">No document selected</div>';const e=v.document;return`
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-start justify-between">
          <div class="flex-1 min-w-0">
            <h3 class="text-lg font-semibold text-gray-900 dark:text-white truncate" id="preview-title">${R(e.title)}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 truncate">${R(e.file_path)}</p>
            <div class="mt-1 flex items-center space-x-2">
              <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300">
                ${R(e.doc_type)}
              </span>
              <span class="text-xs text-gray-500 dark:text-gray-400">${R(e.id)}</span>
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
              ${Te(e.links_to||[])}
            </nav>
          </div>
          <div>
            <h4 class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2" id="linked-from-heading">Linked from</h4>
            <nav aria-labelledby="linked-from-heading">
              ${Te(e.linked_from||[])}
            </nav>
          </div>
        </div>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-auto p-4 font-mono bg-white dark:bg-gray-900" role="region" aria-label="Document content">
        ${e.content?_t(e.content,v.highlightLine):'<p class="text-gray-500 dark:text-gray-400">No content available</p>'}
      </div>
    </div>
  `}function Ie(){if(!m)return;const e=m.querySelector("#preview-panel-content");e&&(e.innerHTML=Ge(),We(),v.highlightLine!==null&&!v.loading&&setTimeout(()=>{const t=document.getElementById("highlighted-line");t==null||t.scrollIntoView({behavior:"smooth",block:"center"})},100))}function We(){var e,t;(e=document.getElementById("preview-close-btn"))==null||e.addEventListener("click",pe),(t=document.getElementById("preview-close-error"))==null||t.addEventListener("click",pe),document.querySelectorAll(".preview-link-btn").forEach(r=>{r.addEventListener("click",s=>{const n=s.currentTarget.dataset.docId;n&&G(n)})})}async function Rt(e){v.loading=!0,v.error=null,v.document=null,Ie();try{const t=await k.getDocument(e),r=await k.getDocumentLinks(e);v.document={...t,links_to:r.links_to,linked_from:r.linked_from}}catch(t){t instanceof S?v.error=t.message:v.error="Failed to load document"}finally{v.loading=!1,Ie()}}function G(e,t){v.highlightLine=t??null,K||(qt(),K=!0),Rt(e)}function pe(){m&&(m.classList.add("translate-x-full"),setTimeout(()=>{m==null||m.remove(),m=null},300)),K=!1,v.document=null,v.error=null,v.highlightLine=null}function qt(){m==null||m.remove(),m=document.createElement("div"),m.id="document-preview-panel",m.className=`
    fixed top-0 right-0 h-full w-full sm:w-[480px] lg:w-[560px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g," "),m.setAttribute("role","dialog"),m.setAttribute("aria-modal","true"),m.setAttribute("aria-labelledby","preview-title"),m.innerHTML=`
    <div id="preview-panel-content" class="h-full flex flex-col">
      ${Ge()}
    </div>
  `,document.body.appendChild(m),requestAnimationFrame(()=>{m==null||m.classList.remove("translate-x-full")}),We();const e=t=>{t.key==="Escape"&&K&&pe()};document.addEventListener("keydown",e),m._cleanup=()=>{document.removeEventListener("keydown",e)}}function we(){if(m){const e=m._cleanup;e&&e(),m.remove(),m=null}K=!1,v.document=null,v.error=null,v.highlightLine=null}const D=[];let be="toast-container";function Ve(){if(document.getElementById(be))return;const e=document.createElement("div");e.id=be,e.className="fixed bottom-4 right-4 z-50 flex flex-col space-y-2 max-w-sm",e.setAttribute("role","region"),e.setAttribute("aria-label","Notifications"),document.body.appendChild(e)}function ee(e){Ve();const t=`toast-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,r=e.duration??5e3,s={id:t,options:e};return r>0&&(s.timeoutId=window.setTimeout(()=>xe(t),r)),D.push(s),Je(),t}function xe(e){const t=D.findIndex(s=>s.id===e);if(t===-1)return;const r=D[t];r.timeoutId&&clearTimeout(r.timeoutId),D.splice(t,1),Je()}const M={success:(e,t)=>ee({message:e,type:"success",...t}),error:(e,t)=>ee({message:e,type:"error",duration:0,...t}),info:(e,t)=>ee({message:e,type:"info",...t}),warning:(e,t)=>ee({message:e,type:"warning",...t})};function Je(){const e=document.getElementById(be);e&&(e.innerHTML=D.map(t=>Bt(t)).join(""),D.forEach(t=>{const r=document.getElementById(`${t.id}-dismiss`);if(r==null||r.addEventListener("click",()=>xe(t.id)),t.options.action){const s=document.getElementById(`${t.id}-action`);s==null||s.addEventListener("click",()=>{var n;(n=t.options.action)==null||n.onClick(),xe(t.id)})}}))}function Bt(e){const{id:t,options:r}=e,{message:s,type:n="info",action:a}=r,i=jt(n),l=Dt(n);return`
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
        <p class="text-sm font-medium ${i.text}">${Ae(s)}</p>
        ${a?`
          <button
            id="${t}-action"
            class="mt-2 text-sm font-medium ${i.action} hover:underline"
          >
            ${Ae(a.label)}
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
  `}function jt(e){switch(e){case"success":return{bg:"bg-green-50 dark:bg-green-900/30",border:"border-green-200 dark:border-green-800",text:"text-green-800 dark:text-green-200",icon:"text-green-500 dark:text-green-400",action:"text-green-700 dark:text-green-300",dismiss:"text-green-500 dark:text-green-400"};case"error":return{bg:"bg-red-50 dark:bg-red-900/30",border:"border-red-200 dark:border-red-800",text:"text-red-800 dark:text-red-200",icon:"text-red-500 dark:text-red-400",action:"text-red-700 dark:text-red-300",dismiss:"text-red-500 dark:text-red-400"};case"warning":return{bg:"bg-amber-50 dark:bg-amber-900/30",border:"border-amber-200 dark:border-amber-800",text:"text-amber-800 dark:text-amber-200",icon:"text-amber-500 dark:text-amber-400",action:"text-amber-700 dark:text-amber-300",dismiss:"text-amber-500 dark:text-amber-400"};case"info":default:return{bg:"bg-blue-50 dark:bg-blue-900/30",border:"border-blue-200 dark:border-blue-800",text:"text-blue-800 dark:text-blue-200",icon:"text-blue-500 dark:text-blue-400",action:"text-blue-700 dark:text-blue-300",dismiss:"text-blue-500 dark:text-blue-400"}}}function Dt(e){switch(e){case"success":return`
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
      `}}function Ae(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}const c={data:null,repos:[],loading:!0,error:null,filterRepo:"",filterType:"",successMessage:null,bulkMode:!1},Ht=["temporal","conflict","missing","ambiguous","stale","duplicate"];function Q(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}async function z(){c.loading=!0,c.error=null,q();try{const e={};c.filterRepo&&(e.repo=c.filterRepo),c.filterType&&(e.type=c.filterType);const[t,r]=await Promise.all([k.getReviewQueue(e),c.repos.length===0?k.getRepositories():Promise.resolve({repositories:c.repos})]);c.data=t,c.repos=r.repositories}catch(e){e instanceof S?c.error=e.message:c.error="Failed to load review queue"}finally{c.loading=!1,q()}}function zt(e){const t=e.questions.filter(s=>!s.answered).length,r=e.questions.length;return`
    <div class="document-group bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden">
      <div class="px-4 py-3 bg-gray-50 dark:bg-gray-700 border-b border-gray-200 dark:border-gray-600">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-medium text-gray-900 dark:text-white">${Q(e.doc_title)}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400">${Q(e.file_path)}</p>
          </div>
          <div class="flex items-center space-x-3">
            <button
              class="preview-doc-btn text-sm text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 flex items-center space-x-1"
              data-doc-id="${Q(e.doc_id)}"
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
        ${e.questions.map((s,n)=>At(s,e.doc_id,n,{showAnswerForm:!c.bulkMode,showCheckbox:c.bulkMode})).join("")}
      </div>
    </div>
  `}function Pt(){const e=c.repos.map(r=>`<option value="${Q(r.id)}" ${c.filterRepo===r.id?"selected":""}>${Q(r.name)}</option>`).join(""),t=Ht.map(r=>`<option value="${r}" ${c.filterType===r?"selected":""}>${r}</option>`).join("");return`
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
  `}function Ye(){if(!c.data)return"";const e={};for(const r of c.data.documents)for(const s of r.questions)s.answered||(e[s.question_type]=(e[s.question_type]||0)+1);return`
    <div class="flex items-center justify-between text-sm">
      <div class="flex items-center space-x-1">${Object.entries(e).sort((r,s)=>s[1]-r[1]).map(([r,s])=>`${Ke(r)} <span class="ml-1 text-gray-600 dark:text-gray-400">${s}</span>`).join('<span class="mx-2 text-gray-300 dark:text-gray-600">|</span>')||'<span class="text-gray-500 dark:text-gray-400">No pending questions</span>'}</div>
      <div class="text-gray-500 dark:text-gray-400">
        ${c.data.unanswered} pending / ${c.data.total} total
      </div>
    </div>
  `}function q(){const e=document.getElementById("review-queue-content");if(!e)return;const t=document.getElementById("review-summary");t&&c.data&&(t.innerHTML=Ye());const r=document.getElementById("bulk-actions-container");r&&c.data&&c.bulkMode?(r.innerHTML=St(c.data.unanswered),Tt(r,c.data.documents,{onSuccess:Qt,onError:Kt,onSelectionChange:Gt},q)):r&&(r.innerHTML="");const s=document.getElementById("review-message");if(s&&(s.innerHTML=""),c.loading){e.innerHTML=`
      <div class="space-y-4">
        ${le()}
        ${le()}
        ${le()}
      </div>
    `;return}if(c.error){e.innerHTML=oe({title:"Error loading review queue",message:c.error,onRetry:z}),de(z);return}if(!c.data||c.data.documents.length===0){e.innerHTML=`
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No pending review questions</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Run <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase lint --review</code> to generate questions</p>
      </div>
    `;return}const n=[...c.data.documents].sort((a,i)=>{const l=a.questions.filter(L=>!L.answered).length;return i.questions.filter(L=>!L.answered).length-l});e.innerHTML=`
    <div class="space-y-4">
      ${n.map(a=>zt(a)).join("")}
    </div>
  `,c.bulkMode||kt(e,{onSuccess:Nt,onError:Ut}),Ot(e)}function Ot(e){e.querySelectorAll(".preview-doc-btn").forEach(t=>{t.addEventListener("click",r=>{const s=r.currentTarget.dataset.docId;s&&G(s)})}),e.querySelectorAll(".preview-line-btn").forEach(t=>{t.addEventListener("click",r=>{const s=r.currentTarget.dataset.docId,n=r.currentTarget.dataset.lineRef;s&&G(s,n?parseInt(n,10):void 0)})})}function Ft(){const e=document.getElementById("filter-repo"),t=document.getElementById("filter-type");e==null||e.addEventListener("change",r=>{c.filterRepo=r.target.value,z()}),t==null||t.addEventListener("change",r=>{c.filterType=r.target.value,z()})}function Nt(e,t,r){if(c.data){const s=c.data.documents.find(n=>n.doc_id===e);s&&s.questions[t]&&(s.questions[t].answered=!0,s.questions[t].answer=r,c.data.answered++,c.data.unanswered--)}M.success(`Answer submitted for question ${t+1}`),q()}function Ut(e){M.error(`Failed to submit answer: ${e}`),q()}function Qt(e,t){M.success(`Successfully ${t} ${e} question(s)`),z()}function Kt(e){M.error(`Bulk action failed: ${e}`),q()}function Gt(e){}function Wt(){c.bulkMode=!c.bulkMode,c.bulkMode||Qe(),q()}function Vt(){const e=c.bulkMode?"Exit bulk mode":"Bulk actions";return`
    <div class="space-y-4 sm:space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Review Queue</h2>
        <button
          id="bulk-mode-toggle"
          class="inline-flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md ${c.bulkMode?"bg-blue-600 text-white hover:bg-blue-700":"bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600"}"
        >
          ${e}
        </button>
      </div>
      <div id="review-message"></div>
      <div id="bulk-actions-container"></div>
      <div id="review-filters" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${Pt()}
      </div>
      <div id="review-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${Ye()}
      </div>
      <div id="review-queue-content">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading review queue...</p>
        </div>
      </div>
    </div>
  `}function Jt(){Ft(),Yt(),z()}function Yt(){var e;(e=document.getElementById("bulk-mode-toggle"))==null||e.addEventListener("click",Wt)}function Xt(){c.data=null,c.loading=!0,c.error=null,c.successMessage=null,c.bulkMode=!1,wt(),Qe(),we()}function y(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function T(e){const t={merge:"bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200",misplaced:"bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200",duplicate:"bg-rose-100 text-rose-800 dark:bg-rose-900 dark:text-rose-200"},r={merge:"🔗",misplaced:"📁",duplicate:"👥"};return`<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${t[e]}">
    <span class="mr-1">${r[e]}</span>${e}
  </span>`}function Zt(e){return`${Math.round(e*100)}%`}function er(e){return e>=.95?"text-red-600 dark:text-red-400":e>=.9?"text-amber-600 dark:text-amber-400":"text-gray-600 dark:text-gray-400"}function tr(e,t,r={}){const{showDismiss:s=!0,showApprove:n=!0,showCompare:a=!0}=r,i=er(e.similarity);return`
    <div class="suggestion-card merge-card bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4" data-type="merge" data-index="${t}">
      <div class="flex items-start justify-between">
        <div class="flex-1">
          <div class="flex items-center space-x-2 mb-2">
            ${T("merge")}
            <span class="text-sm font-medium ${i}">
              ${Zt(e.similarity)} similar
            </span>
          </div>
          <div class="space-y-2">
            <div class="flex items-center space-x-2">
              <span class="text-gray-500 dark:text-gray-400 text-sm">Doc 1:</span>
              <button
                class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
                data-doc-id="${y(e.doc1_id)}"
              >
                ${y(e.doc1_title)}
              </button>
              <span class="text-gray-400 dark:text-gray-500 text-xs">[${y(e.doc1_id)}]</span>
            </div>
            <div class="flex items-center space-x-2">
              <span class="text-gray-500 dark:text-gray-400 text-sm">Doc 2:</span>
              <button
                class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
                data-doc-id="${y(e.doc2_id)}"
              >
                ${y(e.doc2_title)}
              </button>
              <span class="text-gray-400 dark:text-gray-500 text-xs">[${y(e.doc2_id)}]</span>
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
            data-doc1="${y(e.doc1_id)}"
            data-doc2="${y(e.doc2_id)}"
            title="Compare documents side-by-side"
          >
            Compare
          </button>
          `:""}
          ${n?`
          <button
            class="approve-btn px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            data-type="merge"
            data-doc1="${y(e.doc1_id)}"
            data-doc2="${y(e.doc2_id)}"
            title="Approve merge (requires CLI)"
          >
            Approve
          </button>
          `:""}
          ${s?`
          <button
            class="dismiss-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-type="merge"
            data-doc-id="${y(e.doc1_id)}"
            data-target-id="${y(e.doc2_id)}"
          >
            Dismiss
          </button>
          `:""}
        </div>
        `:""}
      </div>
    </div>
  `}function rr(e,t,r={}){const{showDismiss:s=!0,showApprove:n=!0,showSections:a=!0}=r;return`
    <div class="suggestion-card misplaced-card bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4" data-type="misplaced" data-index="${t}">
      <div class="flex items-start justify-between">
        <div class="flex-1">
          <div class="flex items-center space-x-2 mb-2">
            ${T("misplaced")}
          </div>
          <div class="flex items-center space-x-2 mb-2">
            <button
              class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
              data-doc-id="${y(e.doc_id)}"
            >
              ${y(e.doc_title)}
            </button>
            <span class="text-gray-400 dark:text-gray-500 text-xs">[${y(e.doc_id)}]</span>
          </div>
          <div class="flex items-center space-x-2 text-sm">
            <span class="text-gray-500 dark:text-gray-400">Current type:</span>
            <span class="px-2 py-0.5 bg-gray-100 dark:bg-gray-700 rounded text-gray-700 dark:text-gray-300">${y(e.current_type)}</span>
            <span class="text-gray-400 dark:text-gray-500">→</span>
            <span class="text-gray-500 dark:text-gray-400">Suggested:</span>
            <span class="px-2 py-0.5 bg-green-100 dark:bg-green-900 rounded text-green-700 dark:text-green-300">${y(e.suggested_type)}</span>
          </div>
          <p class="mt-2 text-sm text-gray-600 dark:text-gray-300">
            ${y(e.reason)}
          </p>
        </div>
        ${s||n||a?`
        <div class="flex items-center space-x-2 ml-4">
          ${a?`
          <button
            class="sections-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-doc-id="${y(e.doc_id)}"
            title="View document sections"
          >
            Sections
          </button>
          `:""}
          ${n?`
          <button
            class="approve-btn px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            data-type="misplaced"
            data-doc-id="${y(e.doc_id)}"
            data-suggested-type="${y(e.suggested_type)}"
            title="Approve retype (requires CLI)"
          >
            Approve
          </button>
          `:""}
          ${s?`
          <button
            class="dismiss-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-type="misplaced"
            data-doc-id="${y(e.doc_id)}"
          >
            Dismiss
          </button>
          `:""}
        </div>
        `:""}
      </div>
    </div>
  `}const f={loading:!1,error:null,doc1:null,doc2:null};let W=!1,b=null,J=null,Y=null;function _(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function fe(e){const t=e.split(`
`);let r=0;for(const s of t){const n=s.trim();(n.startsWith("- ")||n.startsWith("* ")||/^\d+\.\s/.test(n))&&r++}return r}function _e(e,t){var n;if(!e)return`
      <div class="flex-1 min-w-0 p-4 bg-gray-50 dark:bg-gray-800/50 rounded-lg">
        <div class="text-center text-gray-500 dark:text-gray-400">
          Loading...
        </div>
      </div>
    `;const r=e.content?fe(e.content):0,s=((n=e.content)==null?void 0:n.split(`
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
  `}function Xe(){var s,n;if(f.loading)return`
      <div class="flex items-center justify-center h-64">
        <div class="text-center">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading documents...</p>
        </div>
      </div>
    `;if(f.error)return`
      <div class="p-4 text-center">
        <p class="text-red-600 dark:text-red-400">${_(f.error)}</p>
        <button id="merge-preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;const e=(s=f.doc1)!=null&&s.content?fe(f.doc1.content):0,t=(n=f.doc2)!=null&&n.content?fe(f.doc2.content):0;return`
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
          ${_e(f.doc1,"Document 1")}
          ${_e(f.doc2,"Document 2")}
        </div>
      </div>

      <!-- Actions -->
      <div class="flex-shrink-0 p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <p class="text-xs text-gray-500 dark:text-gray-400">
            Merge requires CLI: <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize merge ${J||"doc1"} ${Y||"doc2"}</code>
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
  `}function Re(){if(!b)return;const e=b.querySelector("#merge-preview-content");e&&(e.innerHTML=Xe(),Ze())}function Ze(){var e,t,r,s;(e=document.getElementById("merge-preview-close-btn"))==null||e.addEventListener("click",re),(t=document.getElementById("merge-preview-close-error"))==null||t.addEventListener("click",re),(r=document.getElementById("merge-preview-dismiss-btn"))==null||r.addEventListener("click",()=>{re()}),(s=document.getElementById("merge-preview-approve-btn"))==null||s.addEventListener("click",()=>{const n=`factbase organize merge ${J} ${Y}`;navigator.clipboard.writeText(n).then(()=>{const a=document.getElementById("merge-preview-approve-btn");a&&(a.textContent="Copied!",setTimeout(()=>{a.textContent="Copy Command"},2e3))}).catch(()=>{alert(`Run: ${n}`)})})}async function sr(e,t){f.loading=!0,f.error=null,f.doc1=null,f.doc2=null,Re();try{const[r,s]=await Promise.all([k.getDocument(e),k.getDocument(t)]);f.doc1=r,f.doc2=s}catch(r){r instanceof S?f.error=r.message:f.error="Failed to load documents"}finally{f.loading=!1,Re()}}function nr(e,t){J=e,Y=t,W||(ar(),W=!0),sr(e,t)}function re(){b&&(b.classList.add("translate-x-full"),setTimeout(()=>{b==null||b.remove(),b=null},300)),W=!1,f.doc1=null,f.doc2=null,f.error=null,J=null,Y=null}function ar(){b==null||b.remove(),b=document.createElement("div"),b.id="merge-preview-panel",b.className=`
    fixed top-0 right-0 h-full w-full lg:w-[900px] xl:w-[1100px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g," "),b.innerHTML=`
    <div id="merge-preview-content" class="h-full flex flex-col">
      ${Xe()}
    </div>
  `,document.body.appendChild(b),requestAnimationFrame(()=>{b==null||b.classList.remove("translate-x-full")}),Ze();const e=t=>{t.key==="Escape"&&W&&re()};document.addEventListener("keydown",e),b._cleanup=()=>{document.removeEventListener("keydown",e)}}function ir(){if(b){const e=b._cleanup;e&&e(),b.remove(),b=null}W=!1,f.doc1=null,f.doc2=null,f.error=null,J=null,Y=null}const p={loading:!1,error:null,doc:null,sections:[]};let V=!1,x=null,X=null;function B(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function qe(e){const t=e.split(`
`);let r=0;for(const s of t){const n=s.trim();(n.startsWith("- ")||n.startsWith("* ")||/^\d+\.\s/.test(n))&&r++}return r}function or(e){const t=e.split(`
`),r=[];let s="Introduction",n=0,a=1,i=[];for(let l=0;l<t.length;l++){const h=l+1,L=t[l];if(L.startsWith("<!-- factbase:"))continue;const E=dr(L);if(E){if(i.length>0){const C=i.join(`
`).trim();C&&r.push({title:s,level:n,startLine:a,endLine:h-1,content:C,factCount:qe(C)})}s=E.title,n=E.level,a=h,i=[]}else i.push(L)}if(i.length>0){const l=i.join(`
`).trim();l&&r.push({title:s,level:n,startLine:a,endLine:t.length,content:l,factCount:qe(l)})}return r}function dr(e){const t=e.trimStart();if(!t.startsWith("#"))return null;let r=0;for(const n of t)if(n==="#")r++;else break;if(r===0||r>6)return null;const s=t.slice(r).trim();return s?{level:r,title:s}:null}function lr(e,t){const r=e.level>0?`<span class="text-xs text-gray-400 dark:text-gray-500">H${e.level}</span>`:'<span class="text-xs text-gray-400 dark:text-gray-500">Intro</span>',s=e.content.split(`
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
          ${B(e.title)}
        </h4>
      </div>
      <!-- Section Preview -->
      <div class="p-3 font-mono text-xs text-gray-600 dark:text-gray-400 max-h-32 overflow-hidden">
        ${s.map(a=>`<div class="truncate">${B(a)||"&nbsp;"}</div>`).join("")}
        ${n?'<div class="text-gray-400 dark:text-gray-500 mt-1">...</div>':""}
      </div>
    </div>
  `}function et(){if(p.loading)return`
      <div class="flex items-center justify-center h-64">
        <div class="text-center">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading document...</p>
        </div>
      </div>
    `;if(p.error)return`
      <div class="p-4 text-center">
        <p class="text-red-600 dark:text-red-400">${B(p.error)}</p>
        <button id="split-preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;if(!p.doc)return`
      <div class="p-4 text-center text-gray-500 dark:text-gray-400">
        No document loaded
      </div>
    `;const e=p.sections.reduce((r,s)=>r+s.factCount,0),t=p.sections.filter(r=>r.content.length>=50);return`
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
            <h4 class="text-sm font-semibold text-gray-900 dark:text-white">${B(p.doc.title)}</h4>
            <div class="mt-1 flex items-center space-x-2">
              <span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
                ${B(p.doc.doc_type)}
              </span>
              <span class="text-xs text-gray-400 dark:text-gray-500">[${B(p.doc.id)}]</span>
            </div>
          </div>
          <div class="text-right text-sm">
            <div class="text-gray-600 dark:text-gray-300">
              <span class="font-medium">${p.sections.length}</span> sections
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
          ${p.sections.map((r,s)=>lr(r,s)).join("")}
        </div>
        ${p.sections.length===0?`
          <div class="text-center text-gray-500 dark:text-gray-400 py-8">
            No sections found in document
          </div>
        `:""}
      </div>

      <!-- Actions -->
      <div class="flex-shrink-0 p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <p class="text-xs text-gray-500 dark:text-gray-400">
            Split requires CLI: <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize split ${X||"doc_id"}</code>
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
  `}function Be(){if(!x)return;const e=x.querySelector("#split-preview-content");e&&(e.innerHTML=et(),tt())}function tt(){var e,t,r,s;(e=document.getElementById("split-preview-close-btn"))==null||e.addEventListener("click",se),(t=document.getElementById("split-preview-close-error"))==null||t.addEventListener("click",se),(r=document.getElementById("split-preview-dismiss-btn"))==null||r.addEventListener("click",se),(s=document.getElementById("split-preview-copy-btn"))==null||s.addEventListener("click",()=>{const n=`factbase organize split ${X}`;navigator.clipboard.writeText(n).then(()=>{const a=document.getElementById("split-preview-copy-btn");a&&(a.textContent="Copied!",setTimeout(()=>{a.textContent="Copy Command"},2e3))}).catch(()=>{alert(`Run: ${n}`)})})}async function cr(e){p.loading=!0,p.error=null,p.doc=null,p.sections=[],Be();try{const t=await k.getDocument(e);p.doc=t,t.content&&(p.sections=or(t.content))}catch(t){t instanceof S?p.error=t.message:p.error="Failed to load document"}finally{p.loading=!1,Be()}}function ur(e){X=e,V||(gr(),V=!0),cr(e)}function se(){x&&(x.classList.add("translate-x-full"),setTimeout(()=>{x==null||x.remove(),x=null},300)),V=!1,p.doc=null,p.sections=[],p.error=null,X=null}function gr(){x==null||x.remove(),x=document.createElement("div"),x.id="split-preview-panel",x.className=`
    fixed top-0 right-0 h-full w-full sm:w-[480px] lg:w-[560px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g," "),x.innerHTML=`
    <div id="split-preview-content" class="h-full flex flex-col">
      ${et()}
    </div>
  `,document.body.appendChild(x),requestAnimationFrame(()=>{x==null||x.classList.remove("translate-x-full")}),tt();const e=t=>{t.key==="Escape"&&V&&se()};document.addEventListener("keydown",e),x._cleanup=()=>{document.removeEventListener("keydown",e)}}function mr(){if(x){const e=x._cleanup;e&&e(),x.remove(),x=null}V=!1,p.doc=null,p.sections=[],p.error=null,X=null}const d={data:null,repos:[],loading:!0,error:null,filterRepo:"",filterType:"",successMessage:null},pr=["merge","misplaced","duplicate"];function j(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}async function P(){d.loading=!0,d.error=null,ye();try{const e={};d.filterRepo&&(e.repo=d.filterRepo),d.filterType&&(e.type=d.filterType);const[t,r]=await Promise.all([k.getSuggestions(e),d.repos.length===0?k.getRepositories():Promise.resolve({repositories:d.repos})]);d.data=t,d.repos=r.repositories}catch(e){e instanceof S?d.error=e.message:d.error="Failed to load suggestions"}finally{d.loading=!1,ye()}}function br(){const e=d.repos.map(r=>`<option value="${j(r.id)}" ${d.filterRepo===r.id?"selected":""}>${j(r.name)}</option>`).join(""),t=pr.map(r=>`<option value="${r}" ${d.filterType===r?"selected":""}>${r}</option>`).join("");return`
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
  `}function rt(){if(!d.data)return"";const e=d.data.merge.length,t=d.data.misplaced.length,r=d.data.duplicate_entries.length,s=[];return e>0&&s.push(`${T("merge")} <span class="ml-1 text-gray-600 dark:text-gray-400">${e}</span>`),t>0&&s.push(`${T("misplaced")} <span class="ml-1 text-gray-600 dark:text-gray-400">${t}</span>`),r>0&&s.push(`${T("duplicate")} <span class="ml-1 text-gray-600 dark:text-gray-400">${r}</span>`),`
    <div class="flex items-center justify-between text-sm">
      <div class="flex items-center space-x-4">
        ${s.length>0?s.join('<span class="mx-2 text-gray-300 dark:text-gray-600">|</span>'):'<span class="text-gray-500 dark:text-gray-400">No pending suggestions</span>'}
      </div>
      <div class="text-gray-500 dark:text-gray-400">
        ${d.data.total} total suggestion${d.data.total!==1?"s":""}
      </div>
    </div>
  `}function xr(){return!d.data||d.data.merge.length===0||d.filterType&&d.filterType!=="merge"?"":`
    <div class="space-y-4">
      <h3 class="text-lg font-medium text-gray-900 dark:text-white flex items-center space-x-2">
        ${T("merge")}
        <span>Merge Candidates</span>
        <span class="text-sm font-normal text-gray-500 dark:text-gray-400">(${d.data.merge.length})</span>
      </h3>
      <div class="space-y-3">
        ${d.data.merge.map((e,t)=>tr(e,t)).join("")}
      </div>
    </div>
  `}function fr(){return!d.data||d.data.misplaced.length===0||d.filterType&&d.filterType!=="misplaced"?"":`
    <div class="space-y-4">
      <h3 class="text-lg font-medium text-gray-900 dark:text-white flex items-center space-x-2">
        ${T("misplaced")}
        <span>Misplaced Documents</span>
        <span class="text-sm font-normal text-gray-500 dark:text-gray-400">(${d.data.misplaced.length})</span>
      </h3>
      <div class="space-y-3">
        ${d.data.misplaced.map((e,t)=>rr(e,t)).join("")}
      </div>
    </div>
  `}function yr(){if(!d.data||d.data.duplicate_entries.length===0||d.filterType&&d.filterType!=="duplicate")return"";const e=d.data.duplicate_entries.map(t=>{const r=t.entries.map(s=>`<li class="text-sm text-gray-600 dark:text-gray-400">
        <span class="font-medium text-gray-900 dark:text-white">${j(s.doc_title)}</span>
        <span class="text-gray-400 dark:text-gray-500">(${j(s.doc_id)})</span>
        §${j(s.section)} line ${s.line_start}, ${s.facts.length} fact${s.facts.length!==1?"s":""}
      </li>`).join("");return`
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 border border-gray-200 dark:border-gray-700">
        <div class="flex items-center space-x-2 mb-2">
          ${T("duplicate")}
          <span class="font-medium text-gray-900 dark:text-white">${j(t.entity_name)}</span>
          <span class="text-sm text-gray-500 dark:text-gray-400">in ${t.entries.length} documents</span>
        </div>
        <ul class="list-disc list-inside space-y-1">${r}</ul>
      </div>
    `}).join("");return`
    <div class="space-y-4">
      <h3 class="text-lg font-medium text-gray-900 dark:text-white flex items-center space-x-2">
        ${T("duplicate")}
        <span>Duplicate Entries</span>
        <span class="text-sm font-normal text-gray-500 dark:text-gray-400">(${d.data.duplicate_entries.length})</span>
      </h3>
      <div class="space-y-3">${e}</div>
    </div>
  `}function ye(){const e=document.getElementById("organize-content");if(!e)return;const t=document.getElementById("organize-summary");t&&d.data&&(t.innerHTML=rt());const r=document.getElementById("organize-message");if(r&&(r.innerHTML=""),d.loading){e.innerHTML=Oe(4);return}if(d.error){e.innerHTML=oe({title:"Error loading suggestions",message:d.error,onRetry:P}),de(P);return}if(!d.data||d.data.total===0){e.innerHTML=`
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No pending organize suggestions</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Run <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize analyze</code> to detect suggestions</p>
      </div>
    `;return}const s=xr(),n=fr(),a=yr();if(!s&&!n&&!a){e.innerHTML=`
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
  `,hr(e),kr(e)}function vr(){const e=document.getElementById("filter-repo"),t=document.getElementById("filter-type");e==null||e.addEventListener("change",r=>{d.filterRepo=r.target.value,P()}),t==null||t.addEventListener("change",r=>{d.filterType=r.target.value,P()})}function hr(e){e.querySelectorAll(".compare-btn").forEach(t=>{t.addEventListener("click",r=>{const s=r.currentTarget,n=s.dataset.doc1,a=s.dataset.doc2;n&&a&&nr(n,a)})}),e.querySelectorAll(".sections-btn").forEach(t=>{t.addEventListener("click",r=>{const n=r.currentTarget.dataset.docId;n&&ur(n)})}),e.querySelectorAll(".approve-btn").forEach(t=>{t.addEventListener("click",async r=>{const s=r.currentTarget,a=s.dataset.type==="merge"?`To merge these documents, run: factbase organize merge ${s.dataset.doc1} ${s.dataset.doc2}`:`To retype this document, run: factbase organize retype ${s.dataset.docId} --type ${s.dataset.suggestedType}`;alert(a)})}),e.querySelectorAll(".dismiss-btn").forEach(t=>{t.addEventListener("click",async r=>{const s=r.currentTarget,n=s.dataset.type,a=s.dataset.docId,i=s.dataset.targetId;s.textContent="Dismissing...",s.disabled=!0;try{await k.dismissSuggestion(n,a,i),M.success("Suggestion dismissed"),P()}catch(l){l instanceof S?M.error(`Failed to dismiss: ${l.message}`):M.error("Failed to dismiss suggestion"),ye()}})})}function kr(e){e.querySelectorAll(".preview-doc-btn").forEach(t=>{t.addEventListener("click",r=>{const s=r.currentTarget.dataset.docId;s&&G(s)})})}function wr(){return`
    <div class="space-y-4 sm:space-y-6">
      <div class="flex items-center justify-between">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Organize Suggestions</h2>
      </div>
      <div id="organize-message"></div>
      <div id="organize-filters" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${br()}
      </div>
      <div id="organize-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${rt()}
      </div>
      <div id="organize-content">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading suggestions...</p>
        </div>
      </div>
    </div>
  `}function $r(){vr(),P()}function Lr(){d.data=null,d.loading=!0,d.error=null,d.successMessage=null,we(),ir(),mr()}function A(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}function Er(e,t,r={}){const{showCheckbox:s=!1}=r,n=`orphan-${t}-${e.line_number}`,a=e.source_doc?`<span class="text-xs text-gray-500 dark:text-gray-400">from <button class="preview-source-btn text-blue-600 dark:text-blue-400 hover:underline" data-doc-id="${A(e.source_doc)}" data-line="${e.source_line||""}">${A(e.source_doc)}</button>${e.source_line?` line ${e.source_line}`:""}</span>`:"",i=e.answered?`<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300">
        Assigned: ${A(e.answer||"")}
      </span>`:`<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-amber-100 dark:bg-amber-900/30 text-amber-800 dark:text-amber-300">
        Pending
      </span>`,l=s?`<input type="checkbox" id="${n}" class="orphan-checkbox h-4 w-4 text-blue-600 rounded border-gray-300 dark:border-gray-600 dark:bg-gray-700 focus:ring-blue-500" data-repo="${A(t)}" data-line="${e.line_number}">`:"",h=e.answered?"":`<div class="orphan-assign-form mt-3 pt-3 border-t border-gray-200 dark:border-gray-700">
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
          ${h}
        </div>
      </div>
    </div>
  `}const o={data:null,repos:[],loading:!0,error:null,selectedRepo:"",successMessage:null,bulkMode:!1,selectedLines:new Set};function je(e){const t=document.createElement("div");return t.textContent=e,t.innerHTML}async function Sr(){try{const e=await k.getRepositories();o.repos=e.repositories,!o.selectedRepo&&o.repos.length>0&&(o.selectedRepo=o.repos[0].id)}catch(e){e instanceof S?o.error=e.message:o.error="Failed to load repositories"}}async function O(){if(!o.selectedRepo){o.data=null,o.loading=!1,H();return}o.loading=!0,o.error=null,H();try{const e=await k.getOrphans(o.selectedRepo);o.data=e}catch(e){e instanceof S?o.error=e.message:o.error="Failed to load orphans",o.data=null}finally{o.loading=!1,H()}}async function De(e,t){if(o.selectedRepo)try{await k.assignOrphan(o.selectedRepo,e,t),M.success(t==="dismiss"?"Orphan dismissed":`Orphan assigned to ${t}`),await O()}catch(r){throw r instanceof S?r:new Error("Failed to assign orphan")}}async function He(e){if(!o.selectedRepo||o.selectedLines.size===0)return;const t=Array.from(o.selectedLines);let r=0;const s=[];for(const n of t)try{await k.assignOrphan(o.selectedRepo,n,e),r++}catch(a){s.push(`Line ${n}: ${a instanceof Error?a.message:"Unknown error"}`)}r>0&&M.success(e==="dismiss"?`Dismissed ${r} orphan(s)`:`Assigned ${r} orphan(s) to ${e}`),s.length>0&&M.error(`Some assignments failed: ${s.join("; ")}`),o.selectedLines.clear(),o.bulkMode=!1,await O()}function Cr(){const e=o.repos.map(t=>`<option value="${je(t.id)}" ${o.selectedRepo===t.id?"selected":""}>${je(t.name)}</option>`).join("");return`
    <div>
      <label for="repo-select" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Repository</label>
      <select id="repo-select" class="block w-full sm:w-64 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
        ${o.repos.length===0?'<option value="">No repositories</option>':""}
        ${e}
      </select>
    </div>
  `}function st(){return o.data?`
    <div class="flex items-center justify-between text-sm">
      <div class="text-gray-600 dark:text-gray-400">
        ${o.data.unanswered} pending / ${o.data.total} total orphans
      </div>
      ${o.data.answered>0?`<div class="text-green-600 dark:text-green-400">${o.data.answered} assigned</div>`:""}
    </div>
  `:""}function Mr(){return!o.bulkMode||o.selectedLines.size===0?"":`
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
  `}function H(){const e=document.getElementById("orphans-content");if(!e)return;const t=document.getElementById("orphans-summary");t&&o.data&&(t.innerHTML=st());const r=document.getElementById("bulk-actions-container");r&&(r.innerHTML=Mr(),Ir());const s=document.getElementById("orphans-message");if(s&&(s.innerHTML=""),o.loading){e.innerHTML=Oe(3);return}if(o.error){e.innerHTML=oe({title:"Error loading orphans",message:o.error,onRetry:O}),de(O);return}if(!o.selectedRepo){e.innerHTML=`
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
      ${n.map(a=>Er(a,o.selectedRepo,{showCheckbox:o.bulkMode})).join("")}
    </div>
  `,Tr(e)}function Tr(e){e.querySelectorAll(".orphan-assign-btn").forEach(t=>{t.addEventListener("click",async r=>{const s=r.currentTarget,n=parseInt(s.dataset.line||"0",10),a=s.closest(".orphan-card"),i=a==null?void 0:a.querySelector(".orphan-target-input"),l=a==null?void 0:a.querySelector(".orphan-error");if(!(i!=null&&i.value.trim())){l&&(l.textContent='Please enter a document ID or "dismiss"',l.classList.remove("hidden"));return}s.disabled=!0,s.textContent="Assigning...";try{await De(n,i.value.trim())}catch(h){l&&(l.textContent=h instanceof Error?h.message:"Failed to assign",l.classList.remove("hidden")),s.disabled=!1,s.textContent="Assign"}})}),e.querySelectorAll(".orphan-dismiss-btn").forEach(t=>{t.addEventListener("click",async r=>{const s=r.currentTarget,n=parseInt(s.dataset.line||"0",10),a=s.closest(".orphan-card"),i=a==null?void 0:a.querySelector(".orphan-error");s.disabled=!0,s.textContent="Dismissing...";try{await De(n,"dismiss")}catch(l){i&&(i.textContent=l instanceof Error?l.message:"Failed to dismiss",i.classList.remove("hidden")),s.disabled=!1,s.textContent="Dismiss"}})}),e.querySelectorAll(".orphan-checkbox").forEach(t=>{t.addEventListener("change",r=>{const s=r.target,n=parseInt(s.dataset.line||"0",10);s.checked?o.selectedLines.add(n):o.selectedLines.delete(n),H()})}),e.querySelectorAll(".preview-source-btn").forEach(t=>{t.addEventListener("click",r=>{r.preventDefault();const s=r.currentTarget.dataset.docId,n=r.currentTarget.dataset.line;s&&G(s,n?parseInt(n,10):void 0)})})}function Ir(){const e=document.getElementById("bulk-assign-btn"),t=document.getElementById("bulk-dismiss-btn"),r=document.getElementById("bulk-target-input");e==null||e.addEventListener("click",async()=>{const s=r==null?void 0:r.value.trim();if(!s){alert("Please enter a document ID");return}confirm(`Assign ${o.selectedLines.size} orphan(s) to ${s}?`)&&await He(s)}),t==null||t.addEventListener("click",async()=>{confirm(`Dismiss ${o.selectedLines.size} orphan(s)?`)&&await He("dismiss")})}function Ar(){const e=document.getElementById("repo-select");e==null||e.addEventListener("change",t=>{o.selectedRepo=t.target.value,o.selectedLines.clear(),O()})}function _r(){o.bulkMode=!o.bulkMode,o.bulkMode||o.selectedLines.clear(),H()}function Rr(){const e=o.bulkMode?"Exit bulk mode":"Bulk actions";return`
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
        ${Cr()}
      </div>
      <div id="orphans-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${st()}
      </div>
      <div id="orphans-content" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading...</p>
        </div>
      </div>
    </div>
  `}async function qr(){var e;await Sr(),Ar(),(e=document.getElementById("bulk-mode-toggle"))==null||e.addEventListener("click",_r),o.selectedRepo?await O():(o.loading=!1,H())}function Br(){o.data=null,o.loading=!0,o.error=null,o.successMessage=null,o.bulkMode=!1,o.selectedLines.clear(),we()}const jr=".document-group, .suggestion-card, .orphan-card, .question-card";let $=-1,me=null,ae=!1;function Z(){return Array.from(document.querySelectorAll(jr))}function ie(e){const t=Z();if(t.length===0)return;const r=Math.max(0,Math.min(e,t.length-1));$>=0&&$<t.length&&t[$].classList.remove("keyboard-focused"),$=r;const s=t[$];s.classList.add("keyboard-focused"),s.scrollIntoView({behavior:"smooth",block:"nearest"}),s.setAttribute("tabindex","0"),s.focus()}function Dr(){Z().length!==0&&($<0?ie(0):ie($+1))}function Hr(){const e=Z();e.length!==0&&($<0?ie(e.length-1):ie($-1))}function zr(){const e=Z();if($<0||$>=e.length)return;const t=e[$],r=t.querySelector(".preview-doc-btn, .preview-line-btn, .compare-btn, .sections-btn");if(r){r.click();return}const s=t.querySelector('textarea[name="answer"]');if(s){s.focus();return}const n=t.querySelector("button");n&&n.click()}function Pr(){if(ae){ve();return}const e=document.getElementById("document-preview-panel"),t=document.getElementById("merge-preview-panel"),r=document.getElementById("split-preview-panel");if(e&&!e.classList.contains("hidden")){const s=e.querySelector(".close-preview-btn");s==null||s.click();return}if(t&&!t.classList.contains("hidden")){const s=t.querySelector(".close-preview-btn");s==null||s.click();return}if(r&&!r.classList.contains("hidden")){const s=r.querySelector(".close-preview-btn");s==null||s.click();return}Or()}function Or(){Z().forEach(t=>{t.classList.remove("keyboard-focused"),t.removeAttribute("tabindex")}),$=-1}function Fr(){var t,r;if(ae)return;ae=!0;const e=document.createElement("div");e.id="keyboard-help-modal",e.className="fixed inset-0 z-50 flex items-center justify-center bg-black/50",e.setAttribute("role","dialog"),e.setAttribute("aria-modal","true"),e.setAttribute("aria-labelledby","keyboard-help-title"),e.innerHTML=`
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
  `,document.body.appendChild(e),e.addEventListener("click",s=>{s.target===e&&ve()}),(t=document.getElementById("close-help-modal"))==null||t.addEventListener("click",ve),(r=document.getElementById("close-help-modal"))==null||r.focus()}function ve(){const e=document.getElementById("keyboard-help-modal");e&&e.remove(),ae=!1}let te=!1,F=null;function Nr(e){const t=e.target;if(t.tagName==="INPUT"||t.tagName==="TEXTAREA"||t.tagName==="SELECT"){e.key==="Escape"&&(t.blur(),e.preventDefault());return}if(te)switch(te=!1,F&&(clearTimeout(F),F=null),e.key.toLowerCase()){case"d":window.location.hash="#/",e.preventDefault();return;case"r":window.location.hash="#/review",e.preventDefault();return;case"o":window.location.hash="#/organize",e.preventDefault();return}switch(e.key.toLowerCase()){case"j":Dr(),e.preventDefault();break;case"k":Hr(),e.preventDefault();break;case"enter":$>=0&&(zr(),e.preventDefault());break;case"escape":Pr(),e.preventDefault();break;case"?":Fr(),e.preventDefault();break;case"g":te=!0,F=window.setTimeout(()=>{te=!1,F=null},500),e.preventDefault();break}}function Ur(){me||(me=Nr,document.addEventListener("keydown",me))}function Qr(){$=-1}let w=!1;function Kr(e){switch(e){case"/":return Se();case"/review":return Vt();case"/organize":return wr();case"/orphans":return Rr();default:return Se()}}function Gr(e){return he.map(t=>{const r=t.path===e,s="flex items-center space-x-2 px-3 py-2 rounded-md text-sm font-medium transition-colors",n=r?"bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-200":"text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700";return`<a href="#${t.path}" class="${s} ${n}">
      <span>${t.icon}</span>
      <span>${t.title}</span>
    </a>`}).join("")}function Wr(e){return he.map(t=>{const r=t.path===e,s="flex items-center space-x-3 px-4 py-3 text-base font-medium transition-colors",n=r?"bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-200":"text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700";return`<a href="#${t.path}" class="mobile-nav-link ${s} ${n}">
      <span class="text-xl">${t.icon}</span>
      <span>${t.title}</span>
    </a>`}).join("")}function Vr(){return`
    <button id="mobile-menu-btn" class="md:hidden p-2 rounded-md text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700" aria-label="${w?"Close menu":"Open menu"}" aria-expanded="${w}" aria-controls="mobile-nav">
      <svg class="w-6 h-6 ${w?"hidden":"block"}" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16"></path>
      </svg>
      <svg class="w-6 h-6 ${w?"block":"hidden"}" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
      </svg>
    </button>
  `}let ze=null;function Jr(e){e==="/"||e===null?yt():e==="/review"?Xt():e==="/organize"?Lr():e==="/orphans"&&Br()}function Yr(e){e==="/"?ft():e==="/review"?Jt():e==="/organize"?$r():e==="/orphans"&&qr(),Qr()}function nt(e){var s;Jr(ze),ze=e,w=!1;const t=((s=he.find(n=>n.path===e))==null?void 0:s.title)||"Dashboard",r=document.querySelector("#app");r.innerHTML=`
    <a href="#main-content" class="skip-link">Skip to main content</a>
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900">
      <header class="bg-white dark:bg-gray-800 shadow" role="banner">
        <div class="max-w-7xl mx-auto px-4 py-4">
          <div class="flex items-center justify-between">
            <a href="#/" class="text-xl font-bold text-blue-600 dark:text-blue-400" aria-label="Factbase - Go to dashboard">Factbase</a>
            <!-- Desktop navigation -->
            <nav class="hidden md:flex space-x-1" role="navigation" aria-label="Main navigation">
              ${Gr(e)}
            </nav>
            <!-- Mobile menu button -->
            ${Vr()}
          </div>
        </div>
        <!-- Mobile navigation -->
        <nav id="mobile-nav" class="md:hidden ${w?"block":"hidden"} border-t border-gray-200 dark:border-gray-700" role="navigation" aria-label="Mobile navigation">
          <div class="px-2 py-2 space-y-1">
            ${Wr(e)}
          </div>
        </nav>
      </header>
      <main id="main-content" class="max-w-7xl mx-auto px-4 py-6 sm:py-8" role="main" aria-label="${t}">
        ${Kr(e)}
      </main>
    </div>
  `,Xr(),Yr(e)}function Xr(){const e=document.getElementById("mobile-menu-btn");e==null||e.addEventListener("click",()=>{var r,s,n,a;w=!w;const t=document.getElementById("mobile-nav");if(t&&t.classList.toggle("hidden",!w),e){e.setAttribute("aria-expanded",String(w));const i=e.querySelectorAll("svg");(r=i[0])==null||r.classList.toggle("hidden",w),(s=i[0])==null||s.classList.toggle("block",!w),(n=i[1])==null||n.classList.toggle("hidden",!w),(a=i[1])==null||a.classList.toggle("block",w)}}),document.querySelectorAll(".mobile-nav-link").forEach(t=>{t.addEventListener("click",()=>{w=!1})})}Pe.onRouteChange(e=>{nt(e)});Ur();Ve();nt(Pe.getCurrentRoute());
