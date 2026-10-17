import './styles.css';
import {mount} from './state';
const view=mount(document.querySelector<HTMLElement>('#app')!);
void view.refresh();
window.addEventListener('pagehide',()=>view.destroy(),{once:true});
