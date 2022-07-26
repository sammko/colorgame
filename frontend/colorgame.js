import moment from 'moment';

const rounds = [10, 8, 8, 7.5, 7, 6.5, 6, 5.5, 5, 4.5, 4, 3.5, 3];
//const rounds = [0.05,0.05,0.05,0.05,0.05,0.05];
var start_time = moment();
var round_number = 0;
var end_time = start_time;


const assignments =
        {
          0: ["", "", ""],
          1: ["Farby dúhy", "červená, žltá, zelená, modrá", "1.jpg"],
          2: ["Farby CMYK", "žltá, tyrkysová, magenta, čierna", ""],
          3: ["Farby listov na jeseň", "zelená, červená, žltá, magenta", "3.jpg"],
          4: ["Farby tričiek vedúcich na tejto hre", "TODO", ""],
          5: ["Farby RGB", "červená, zelená, modrá", "5.jpg"],
          6: ["Šachovnica", "čierna, biela", "6.jpg"],
          7: ["Také farby ako má Viki na ponožkách " +
          "a Hovorca na tričku", "tyrkysová, magenta", ""],
          8: ["Každý člen družinky má inú farbu", "", ""],
          9: ["Farby slovenskej vlajky", "biela, modrá, červená", "9.jpg"],
          10: ["Farby Linux loga", "čierna, biela, žltá", "10.jpg"],
          11: ["Farby vodnej hladiny", "modrá, tyrkysová, zelená", "11.jpeg"],
          12: ["Čo najunikátnejšia farba na sústredku", "", ""]
        }

const $e = (elName, elClass, elParent) => {
  let e = document.createElement (elName);
  if (elClass) e.classList.add (elClass);
  if (elParent) elParent.appendChild (e);
  return e;
}

function clear_tables()
{
  for(let i=1; i<=6; i++)
  {
    let table = document.getElementById("barcode-colors-table"+(i).toString());
    let tbody = table.firstChild
    while(tbody.childElementCount > 1) tbody.removeChild(tbody.lastChild);
  }
}

async function load_state()
{
  const response = await fetch("http://localhost:8000/current");
  if (!response.ok) {
    throw new Error(`Error fetching current state: ${response.status} - ${response.statusText}`)
  } else {
    let data = await response.json();
    clear_tables();
    let empty = true;
    for (const [key, value] of Object.entries(data)) {
      let table = document.getElementById("barcode-colors-table"+(parseInt((key-1)/8+1)).toString());
      let row = $e('tr', '', table.firstChild);
      let bar = $e('td', 'barcode', row);
      bar.innerHTML = key;
      let wrapper = $e('td', 'wrap', row)
      let color = $e('div', value, wrapper);
      color.classList += ' color_square'
      empty = false;
    }
    return empty;
  }
}

function create_barcodes(n=1) {
  for (let i = 0; i < n; i++) {
      console.log(i);
      fetch('http://localhost:8000/init', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          "barcode": (i+1).toString(),
          "timestamp": "2022-07-11T18:15:00.00Z",
          "color": "blue"

        })
      })
              .then(res => console.log(res));
  }
}

function parse_time(seconds)
{
  var mins = parseInt(seconds/60);
  var secs = seconds%60;
  return (mins<10?"0":"") + mins.toString() + ":" + (secs<10?"0":"") + parseInt(secs);
}

function update_timer()
{
  document.getElementById("timer").innerHTML = parse_time((end_time - moment())/1000)
  if((end_time - moment() < 0))
  {
    spustiKolo(round_number);
    round_number++;
  }
}

function zahrajKola(zoznam) {
  if(zoznam.length == 0)
    return;
  var audio = new Audio('audio/'+zoznam[0]+'.mp3');
  audio.onended = function() {
    zahrajKola(zoznam.slice(1,undefined));
  };
  audio.play();
}

function zahlasKolo(x) {
  var finalnyZoznam = ["bell"];
  if(x == 0)
    finalnyZoznam.push("0");
  else {
    let zoznam = [100,90,80,70,60,50,40,30,20,19,18,17,16,15,14,13,12,11,10,9,8,7,6,5,4,3,2,1];
    zoznam.forEach((a)=> {if(x>=a) {finalnyZoznam.push(""+a); x-=a;}});
  }

  finalnyZoznam.push("kolo");
  zahrajKola(finalnyZoznam);
}

function spustiKolo(x)
{
  if(x >= rounds.length)
  {
      clearInterval(timeUpdater);
      clearInterval(stateUpdater);
      return;
  }

  zahlasKolo(x);
  document.getElementById("round").innerHTML = x + ". kolo";
  document.getElementById("as_header").innerHTML = assignments[x][0];
  document.getElementById("as_text").innerHTML = assignments[x][1];
  if( assignments[x][2] != "")
    document.getElementById("as_img").src =  "assignments/" + assignments[x][2];
  else
    document.getElementById("as_img").src =  "";
  start_time = moment();
  end_time = start_time + 1000*(rounds[x]*60);
}

load_state().then(empty => {
  if (empty) {
    create_barcodes(48);
    load_state()
  }
})

var timeUpdater = setInterval(update_timer, 100);
var stateUpdater = setInterval(load_state, 1000);