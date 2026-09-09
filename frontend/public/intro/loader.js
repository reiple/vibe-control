/* Reference: https://www.spasoje.dev/ — independent JavaScript implementation.
 * Same GSAP RoughEase parameters and desktop/mobile timing as the reference.
 * Random intermediate values intentionally differ on every playback.
 */
(() => {
  const $ = selector => document.querySelector(selector);
  const loader = $('#loader'), hero = $('#hero'), replay = $('#replay');
  const digits = [...document.querySelectorAll('.loader-number p')];
  gsap.registerPlugin(RoughEase);
  let context;
  function setDigits(text) { [...text].forEach((char,i) => { digits[i].textContent=char; }); }

  function revealHero(mobile) {
    loader.hidden=true; hero.hidden=false;
    const grid=$('.hero-grid'); grid.replaceChildren();
    const chars=[...'VIBE-CONTROL/NOTHANGTON'];
    // Deliberate empty grid cells preserve the reference's scattered composition.
    const desktop=[
      [1,1],[2,1],[5,1],[6,1],[10,1],
      [3,2],[4,2],[5,2],[7,2],[8,2],[9,2],[10,2],
      [1,3],[3,3],[4,3],[6,3],[7,3],[10,3],
      [2,4],[3,4],[7,4],[8,4],[9,4]
    ];
    const small=[
      [1,1],[2,1],[5,1],[6,1],
      [1,2],[3,2],[4,2],[6,2],
      [2,3],[3,3],[5,3],[6,3],
      [1,4],[3,4],[4,4],[6,4],
      [1,5],[2,5],[5,5],[6,5],
      [2,6],[4,6],[5,6]
    ];
    grid.setAttribute('aria-label','VIBE-CONTROL / NOTHANGTON');
    const cells=chars.map((char,i)=>{
      const cell=document.createElement('div');
      cell.className='hero-char';
      cell.setAttribute('aria-hidden','true');
      cell.style.setProperty('--column',desktop[i][0]);
      cell.style.setProperty('--row',desktop[i][1]);
      cell.style.setProperty('--mobile-column',small[i][0]);
      cell.style.setProperty('--mobile-row',small[i][1]);
      cell.textContent=char; grid.append(cell);
      return {cell,char,initial:false};
    });
    const others=cells.filter(item=>!item.initial);
    for(let i=others.length-1;i>0;i--){const j=Math.floor(Math.random()*(i+1));[others[i],others[j]]=[others[j],others[i]];}
    const alphabet='ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
    others.forEach(({cell,char},i)=>{
      const sequence=Array.from({length:25},()=>alphabet[Math.floor(Math.random()*alphabet.length)]).concat(char);
      const state={value:0};
      gsap.to(state,{value:25,duration:.85,delay:i*.04,ease:'power1.out',onStart:()=>{cell.style.opacity=1;},onUpdate:()=>{cell.textContent=sequence[Math.floor(state.value)];}});
    });
    gsap.delayedCall(1.6,()=>{replay.hidden=false;});
    window.dispatchEvent(new CustomEvent('loader:complete'));
  }

  function play() {
    context?.revert(); loader.hidden=false; hero.hidden=true; replay.hidden=true; setDigits('000');
    const mobile=matchMedia('(max-width:991px)').matches;
    context=gsap.context(()=>{
      gsap.set('.loader-arrow,.loader-percent,.loader-text,.loader-logo',{opacity:1});
      gsap.set('.loader-slash',{opacity:0,clearProps:'gridColumn'});
      gsap.set('.loader-number',{clearProps:'gridColumn'});
      const progress={value:0};
      gsap.to(progress,{
        value:100,duration:2,delay:.3,
        ease:RoughEase.config({template:gsap.parseEase('none'),strength:1,points:20,taper:'out',randomize:true,clamp:true}),
        onUpdate:()=>setDigits(Math.floor(progress.value).toString().padStart(3,'0')),
        onComplete:()=>{
          const timeline=gsap.timeline();
          timeline.set('.loader-arrow,.loader-percent,.loader-text,.loader-logo',{opacity:0},.3);
          if(mobile){
            timeline.set('.loader-number3',{gridColumn:'6'},.35)
              .set('.loader-number2',{gridColumn:'5'},.4)
              .set('.loader-number1',{gridColumn:'4'},.45)
              .set('.loader-slash',{opacity:1,gridColumn:'3'},.55);
          }else timeline.set('.loader-slash',{opacity:1},.3);
          const morph={value:0};
          timeline.to(morph,{value:1,duration:.32,ease:'none',onUpdate:()=>setDigits(morph.value<.33?'D00':morph.value<.66?'DE0':'DEV'),onComplete:()=>revealHero(mobile)},mobile?.6:.5);
        }
      });
    });
  }
  replay.addEventListener('click',play);
  window.spasojeLoader={play,destroy(){context?.revert();loader.hidden=true;}};
  document.fonts.ready.then(play);
})();
