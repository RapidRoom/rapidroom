"""Synthetic native-app regression for creating a mask on an existing row."""


def run_mask_drop_checks(smoke, wait_for):
    smoke.execute("""const e=document.querySelector('[data-tab-id="layout-tab-masks"]');
      if(!e)throw Error('Masks panel tab missing');e.click();return true;""")
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=button][aria-describedby]')]"
        ".some(e=>e.textContent.trim()==='Radial');"), "Radial mask creation tile")
    smoke.execute("""const e=[...document.querySelectorAll('[role=button][aria-describedby]')]
      .find(e=>e.textContent.trim()==='Radial');if(!e)throw Error('Radial creation tile missing');
      e.click();return true;""")

    row_script = """function fiberFor(node){const key=Object.keys(node).find(k=>k.startsWith('__reactFiber$'));
      return key?node[key]:null;}
      function rowProps(node){let f=fiberFor(node);while(f){const p=f.memoizedProps;
        if(p?.subMask?.id&&typeof p.containerId==='string'&&typeof p.onSelect==='function')return p;
        f=f.return;}return null;}
      return [...document.querySelectorAll('[role=button][aria-describedby]')].map(rowProps)
        .filter(Boolean).map(p=>({id:p.subMask.id,type:p.subMask.type,mode:p.subMask.mode,
          name:p.subMask.name||p.subMask.type,containerId:p.containerId}));"""
    wait_for(lambda: len(smoke.execute(row_script)) == 1, "one initial sub-mask row")

    context = smoke.execute("""function fiberFor(node){const key=Object.keys(node).find(k=>k.startsWith('__reactFiber$'));
      return key?node[key]:null;}
      const row=[...document.querySelectorAll('[role=button][aria-describedby]')]
        .find(e=>{let f=fiberFor(e);while(f){const p=f.memoizedProps;
          if(p?.subMask?.id&&typeof p.containerId==='string'&&typeof p.onSelect==='function')return p.subMask.type==='radial';
          f=f.return;}return false;});
      if(!row)throw Error('Initial Radial row missing');
      let f=fiberFor(row),props=null,dropHandler=null;
      for(let p=f;p;p=p.return){
        const value=p.memoizedProps;
        if(!props&&value?.subMask?.id&&typeof value.containerId==='string'&&typeof value.onSelect==='function')props=value;
        if(!dropHandler&&typeof p.memoizedProps?.onDragEnd==='function')dropHandler=p.memoizedProps.onDragEnd;
      }
      if(!props||!dropHandler)throw Error('Masks DnD handler missing');
      dropHandler({active:{id:'create-linear',data:{current:{type:'Creation',maskType:'linear'}}},
        over:{id:props.subMask.id,data:{current:{type:'SubMask',item:props.subMask,parentId:props.containerId}}}});
      return {targetId:props.subMask.id,containerId:props.containerId};""")

    # Fire the app's current animation-completion callback to commit the pending
    # create action without relying on physical pointer hardware.
    smoke.execute("""setTimeout(()=>{const id=arguments[0];
      function fiberFor(node){const key=Object.keys(node).find(k=>k.startsWith('__reactFiber$'));
        return key?node[key]:null;}
      let callback=null;for(const node of document.querySelectorAll('[role=button][aria-describedby]')){
        let row=fiberFor(node),props=null;for(let f=row;f;f=f.return){
          const p=f.memoizedProps;
          if(!props&&p?.subMask?.id&&typeof p.containerId==='string'&&typeof p.onSelect==='function')props=p;
        }
        if(props?.subMask.id===id){for(let f=row;f;f=f.return){
          if(typeof f.memoizedProps?.onExitComplete==='function'){callback=f.memoizedProps.onExitComplete;break;}
        }break;}
      }
      if(callback)callback();},100);return true;""", [context["targetId"]])

    def read_two_rows():
        value = smoke.execute(row_script)
        return value if len(value) == 2 else None

    rows = wait_for(read_two_rows, "created Linear mask inserted at the target", 15)
    if [row["type"] for row in rows] != ["linear", "radial"]:
        raise RuntimeError("Dropped mask was not inserted before its target: " + str(rows))
    if rows[0]["mode"] != "additive":
        raise RuntimeError("Dropped mask did not use additive mode: " + str(rows))
    smoke.step("mask creation drop uses additive target insertion", {
        "target_id": context["targetId"],
        "container_id": context["containerId"],
        "result": [{"type": row["type"], "mode": row["mode"]} for row in rows],
        "event": "synthetic DndContext onDragEnd in the real native app",
    })
